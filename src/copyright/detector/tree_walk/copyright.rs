// SPDX-FileCopyrightText: Provenant contributors
// SPDX-License-Identifier: Apache-2.0

use crate::copyright::types::{
    AuthorDetection, CopyrightDetection, HolderDetection, ParseNode, PosTag, Token, TreeLabel,
};
use crate::models::LineNumber;

use crate::copyright::detector;

fn mpl_portions_created_prefix_tokens<'a>(
    tree: &'a [ParseNode],
    idx: usize,
    copyright_node: &'a ParseNode,
    trailing_tokens: &[&'a Token],
) -> Option<Vec<&'a Token>> {
    let leaves = detector::token_utils::collect_all_leaves(copyright_node);
    let first = *leaves.first()?;
    if first.tag != PosTag::Copy || !first.value.eq_ignore_ascii_case("copyright") {
        return None;
    }

    let mut combined = leaves;
    combined.extend_from_slice(trailing_tokens);

    let has_initial = combined
        .iter()
        .any(|t| t.value.eq_ignore_ascii_case("initial"));
    let has_developer = combined.iter().any(|t| {
        t.value
            .as_str()
            .get(0.."developer".len())
            .is_some_and(|p| p.eq_ignore_ascii_case("developer"))
    });
    if !(has_initial && has_developer) {
        return None;
    }

    let line = first.start_line;
    let mut prev_rev: Vec<&Token> = Vec::with_capacity(7);
    let mut j = idx;
    while j > 0 && prev_rev.len() < 7 {
        j -= 1;
        let leaves = detector::token_utils::collect_all_leaves(&tree[j]);
        for &t in leaves.iter().rev() {
            if t.start_line != line {
                continue;
            }
            prev_rev.push(t);
            if prev_rev.len() == 7 {
                break;
            }
        }
    }

    if prev_rev.len() != 7 {
        return None;
    }
    prev_rev.reverse();

    let values: Vec<&str> = prev_rev.iter().map(|t| t.value.as_str()).collect();
    let matches = values[0].eq_ignore_ascii_case("portions")
        && values[1].eq_ignore_ascii_case("created")
        && values[2].eq_ignore_ascii_case("by")
        && values[3].eq_ignore_ascii_case("the")
        && values[4].eq_ignore_ascii_case("initial")
        && values[5].eq_ignore_ascii_case("developer")
        && values[6].eq_ignore_ascii_case("are");

    matches.then_some(prev_rev)
}

fn single_portions_prefix_token<'a>(
    tree: &'a [ParseNode],
    idx: usize,
    copyright_node: &'a ParseNode,
) -> Option<&'a Token> {
    let first = *detector::token_utils::collect_all_leaves(copyright_node).first()?;
    if idx == 0 {
        return None;
    }

    if first.tag == PosTag::Copy && first.value.eq_ignore_ascii_case("copyright") {
        let ParseNode::Leaf(prev) = &tree[idx - 1] else {
            return None;
        };
        return (prev.tag == PosTag::Portions && prev.start_line == first.start_line)
            .then_some(prev);
    }

    if first.tag == PosTag::Copy && first.value.eq_ignore_ascii_case("(c)") && idx >= 2 {
        let ParseNode::Leaf(prev_copy) = &tree[idx - 1] else {
            return None;
        };
        if prev_copy.tag != PosTag::Copy
            || !prev_copy.value.eq_ignore_ascii_case("copyright")
            || prev_copy.start_line != first.start_line
        {
            return None;
        }

        let ParseNode::Leaf(prev_portions) = &tree[idx - 2] else {
            return None;
        };
        return (prev_portions.tag == PosTag::Portions
            && prev_portions.start_line == first.start_line)
            .then_some(prev_portions);
    }

    None
}

pub fn extract_from_tree_nodes(
    tree: &[ParseNode],
    allow_not_copyrighted_prefix: bool,
) -> (
    Vec<CopyrightDetection>,
    Vec<HolderDetection>,
    Vec<AuthorDetection>,
) {
    let mut copyrights: Vec<CopyrightDetection> = Vec::new();
    let mut holders: Vec<HolderDetection> = Vec::new();
    let mut authors: Vec<AuthorDetection> = Vec::new();

    let group_has_copyright = tree.iter().any(|n| {
        matches!(
            n.label(),
            Some(TreeLabel::Copyright) | Some(TreeLabel::Copyright2)
        )
    });

    let mut preceding_year_only_prefix: Option<Vec<&Token>> = None;

    let mut i = 0;
    while i < tree.len() {
        let node = &tree[i];
        let label = node.label();

        if matches!(
            label,
            Some(TreeLabel::Copyright) | Some(TreeLabel::Copyright2)
        ) {
            if preceding_year_only_prefix.is_none()
                && is_year_only_copyright_clause_node(node)
                && let Some(next_node) = tree.get(i + 1)
                && matches!(
                    next_node.label(),
                    Some(TreeLabel::Copyright) | Some(TreeLabel::Copyright2)
                )
                && !is_year_only_copyright_clause_node(next_node)
                && detector::token_utils::collect_all_leaves(node)
                    .first()
                    .is_some_and(|t| {
                        detector::token_utils::collect_all_leaves(next_node)
                            .first()
                            .is_some_and(|n| n.start_line == t.start_line + 1)
                    })
            {
                let leaves = detector::token_utils::collect_filtered_leaves(
                    node,
                    detector::NON_COPYRIGHT_LABELS,
                    detector::NON_COPYRIGHT_POS_TAGS,
                );
                let leaves = detector::token_utils::strip_all_rights_reserved(leaves);
                if !leaves.is_empty() {
                    preceding_year_only_prefix = Some(leaves);
                    i += 1;
                    continue;
                }
            }

            let allow_single_word_contributors = detector::token_utils::collect_all_leaves(node)
                .iter()
                .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr));
            let prefix_token = get_orphaned_copy_prefix(tree, i);
            let not_prefix = get_orphaned_not_prefix(tree, i, node, allow_not_copyrighted_prefix);
            let (mut trailing_tokens, mut skip) = collect_trailing_orphan_tokens(node, tree, i + 1);
            let mut trailing_copyright_only_tokens: Vec<&Token> = Vec::new();

            if trailing_tokens.is_empty() {
                let last_line = detector::token_utils::collect_all_leaves(node)
                    .last()
                    .map(|t| t.start_line);
                if let Some(last_line) = last_line {
                    let mut merged = false;

                    for offset in 1..=6 {
                        let idx = i + offset;
                        if idx >= tree.len() {
                            break;
                        }
                        let leaves = detector::token_utils::collect_all_leaves(&tree[idx]);
                        if leaves.first().is_none_or(|t| t.start_line != last_line) {
                            break;
                        }
                        let comma_boundary = if last_leaf_ends_with_comma(node) {
                            true
                        } else {
                            ((i + 1)..idx).any(|k| {
                                detector::token_utils::collect_all_leaves(&tree[k])
                                    .iter()
                                    .any(|t| {
                                        t.value == ","
                                            || t.tag == PosTag::Cc
                                            || t.value.ends_with(',')
                                    })
                            })
                        };
                        if !comma_boundary {
                            continue;
                        }

                        if is_year_only_copyright_clause_node(&tree[idx]) {
                            let combined: Vec<&Token> = tree
                                .iter()
                                .take(idx + 1)
                                .skip(i + 1)
                                .flat_map(detector::collect_all_leaves)
                                .collect();
                            trailing_copyright_only_tokens = combined;
                            skip = idx - (i + 1) + 1;
                            merged = true;
                            break;
                        }

                        if let ParseNode::Leaf(token) = &tree[idx]
                            && token.tag == PosTag::Copy
                            && token.value.eq_ignore_ascii_case("copyright")
                        {
                            let (clause_tokens, clause_skip) =
                                collect_following_copyright_clause_tokens(tree, idx, last_line);
                            if clause_tokens.is_empty() {
                                continue;
                            }

                            let mut combined: Vec<&Token> = tree
                                .iter()
                                .take(idx)
                                .skip(i + 1)
                                .flat_map(detector::collect_all_leaves)
                                .collect();
                            combined.extend(clause_tokens);
                            trailing_copyright_only_tokens = combined;
                            skip = (idx - (i + 1)) + clause_skip;
                            merged = true;
                            break;
                        }
                    }

                    if !merged
                        && last_leaf_ends_with_comma(node)
                        && i + 1 < tree.len()
                        && let ParseNode::Leaf(token) = &tree[i + 1]
                        && token.start_line == last_line
                    {
                        let is_comma_separated_holder_leaf =
                            matches!(token.tag, PosTag::MixedCap | PosTag::Comp)
                                || (matches!(token.tag, PosTag::Caps | PosTag::Nnp)
                                    && token.value.contains('-'));
                        if is_comma_separated_holder_leaf {
                            trailing_tokens.push(token);
                            skip = 1;
                        }
                    }
                }
            }

            if !trailing_tokens.is_empty() {
                let last_line = detector::token_utils::collect_all_leaves(node)
                    .last()
                    .map(|t| t.start_line);
                let last_token_has_comma = trailing_tokens.last().is_some_and(|t| {
                    t.value.ends_with(',') || t.value == "," || t.tag == PosTag::Cc
                });

                if last_token_has_comma && let Some(last_line) = last_line {
                    let after_idx = i + 1 + skip;
                    for clause_offset in 0..=2 {
                        let idx = after_idx + clause_offset;
                        if idx >= tree.len() {
                            break;
                        }
                        let leaves = detector::token_utils::collect_all_leaves(&tree[idx]);
                        if leaves.first().is_none_or(|t| t.start_line != last_line) {
                            break;
                        }

                        if is_year_only_copyright_clause_node(&tree[idx]) {
                            trailing_copyright_only_tokens
                                .extend(detector::token_utils::collect_all_leaves(&tree[idx]));
                            skip += clause_offset + 1;
                            break;
                        }

                        if let ParseNode::Leaf(token) = &tree[idx]
                            && token.tag == PosTag::Copy
                            && token.value.eq_ignore_ascii_case("copyright")
                        {
                            let (clause_tokens, clause_skip) =
                                collect_following_copyright_clause_tokens(tree, idx, last_line);
                            if !clause_tokens.is_empty() {
                                trailing_copyright_only_tokens.extend(clause_tokens);
                                skip += clause_offset + clause_skip;
                            }
                            break;
                        }
                    }
                }
            }
            let mpl_prefix = mpl_portions_created_prefix_tokens(tree, i, node, &trailing_tokens);
            let portions_prefix = single_portions_prefix_token(tree, i, node);

            if trailing_tokens.is_empty() && trailing_copyright_only_tokens.is_empty() {
                let has_holder = detector::token_utils::build_holder_from_node(
                    node,
                    detector::NON_HOLDER_LABELS,
                    detector::NON_HOLDER_POS_TAGS,
                )
                .is_some()
                    || detector::token_utils::build_holder_from_node(
                        node,
                        detector::NON_HOLDER_LABELS_MINI,
                        detector::NON_HOLDER_POS_TAGS_MINI,
                    )
                    .is_some();

                if !has_holder
                    && is_year_only_copyright_clause_node(node)
                    && let Some((cr_det, holder_det)) =
                        merge_year_only_copyright_clause_with_preceding_copyrighted_by(
                            tree,
                            i,
                            prefix_token,
                            portions_prefix,
                            mpl_prefix.as_deref(),
                        )
                {
                    copyrights.push(cr_det);
                    holders.push(holder_det);
                    i += 1;
                    continue;
                }

                if !has_holder
                    && i + 1 < tree.len()
                    && matches!(tree[i + 1], ParseNode::Leaf(ref t) if t.tag == PosTag::Uni)
                    && has_name_tree_within(tree, i + 2, 2)
                {
                    let mut cr_tokens: Vec<&Token> = Vec::new();
                    if let Some(prefix) = prefix_token {
                        cr_tokens.push(prefix);
                    }
                    if let Some(prefix) = portions_prefix {
                        cr_tokens.push(prefix);
                    }
                    if let Some(prefix) = mpl_prefix.as_ref() {
                        cr_tokens.extend(prefix.iter().copied());
                    }
                    let node_leaves = detector::token_utils::collect_filtered_leaves(
                        node,
                        detector::NON_COPYRIGHT_LABELS,
                        detector::NON_COPYRIGHT_POS_TAGS,
                    );
                    let node_leaves = detector::token_utils::strip_all_rights_reserved(node_leaves);
                    cr_tokens.extend(&node_leaves);

                    let mut extra_skip = 0;
                    let mut j = i + 1;
                    while j < tree.len()
                        && !is_orphan_boundary(&tree[j])
                        && is_orphan_continuation(&tree[j])
                    {
                        let leaves = detector::token_utils::collect_all_leaves(&tree[j]);
                        cr_tokens.extend(leaves);
                        j += 1;
                        extra_skip += 1;
                    }
                    let cr_tokens = detector::token_utils::strip_all_rights_reserved(cr_tokens);
                    if let Some(det) =
                        detector::token_utils::build_copyright_from_tokens(&cr_tokens)
                    {
                        copyrights.push(det);
                    }

                    let mut holder_tokens: Vec<&Token> = Vec::new();
                    let node_holder_leaves = detector::token_utils::collect_holder_filtered_leaves(
                        node,
                        detector::NON_HOLDER_LABELS,
                        detector::NON_HOLDER_POS_TAGS,
                    );
                    let node_holder_leaves =
                        detector::token_utils::strip_all_rights_reserved(node_holder_leaves);
                    holder_tokens.extend(&node_holder_leaves);
                    let mut k = i + 1;
                    while k < j {
                        let leaves = detector::token_utils::collect_all_leaves(&tree[k]);
                        holder_tokens.extend(leaves);
                        k += 1;
                    }
                    let holder_tokens =
                        detector::token_utils::strip_all_rights_reserved(holder_tokens);
                    if let Some(det) = detector::token_utils::build_holder_from_tokens(
                        &holder_tokens,
                        allow_single_word_contributors,
                    ) {
                        holders.push(det);
                    }

                    i += extra_skip;
                    i += 1;
                    continue;
                }

                if !has_holder
                    && i + 1 < tree.len()
                    && tree[i + 1].label() == Some(TreeLabel::Author)
                    && let Some((cr_det, h_det, skip)) =
                        merge_copyright_with_following_author(node, prefix_token, tree, i + 1)
                {
                    copyrights.push(cr_det);
                    if let Some(h) = h_det {
                        holders.push(h);
                    }
                    i += skip + 1;
                    i += 1;
                    continue;
                }

                if !has_holder && i + 1 < tree.len() {
                    let copyright_ends_with_year = {
                        let leaves = detector::token_utils::collect_all_leaves(node);
                        leaves.last().is_some_and(|t| {
                            matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr)
                        })
                    };
                    let next_node = &tree[i + 1];
                    let next_line_ok = {
                        let last_line = detector::token_utils::collect_all_leaves(node)
                            .last()
                            .map(|t| t.start_line);
                        let first_next_line = detector::token_utils::collect_all_leaves(next_node)
                            .first()
                            .map(|t| t.start_line);
                        last_line.is_some_and(|l| first_next_line == Some(l + 1))
                    };
                    let next_is_holderish = match next_node {
                        ParseNode::Tree { label, .. } => matches!(
                            label,
                            TreeLabel::Name
                                | TreeLabel::NameCaps
                                | TreeLabel::NameYear
                                | TreeLabel::NameEmail
                                | TreeLabel::Company
                                | TreeLabel::AndCo
                                | TreeLabel::DashCaps
                        ),
                        ParseNode::Leaf(t) => matches!(
                            t.tag,
                            PosTag::Nnp
                                | PosTag::Caps
                                | PosTag::Comp
                                | PosTag::MixedCap
                                | PosTag::Uni
                                | PosTag::Pn
                                | PosTag::Email
                        ),
                    };

                    if copyright_ends_with_year && next_line_ok && next_is_holderish {
                        let name_node = next_node;
                        let mut cr_tokens: Vec<&Token> =
                            preceding_year_only_prefix.take().unwrap_or_default();
                        if let Some(prefix) = prefix_token {
                            cr_tokens.push(prefix);
                        }
                        if let Some(prefix) = portions_prefix {
                            cr_tokens.push(prefix);
                        }
                        if let Some(prefix) = mpl_prefix.as_ref() {
                            cr_tokens.extend(prefix.iter().copied());
                        }
                        let node_leaves = detector::token_utils::collect_filtered_leaves(
                            node,
                            detector::NON_COPYRIGHT_LABELS,
                            detector::NON_COPYRIGHT_POS_TAGS,
                        );
                        let node_leaves =
                            detector::token_utils::strip_all_rights_reserved(node_leaves);
                        cr_tokens.extend(&node_leaves);

                        let name_leaves = detector::token_utils::collect_all_leaves(name_node);
                        let mut holder_tokens: Vec<&Token> = name_leaves.clone();
                        cr_tokens.extend(&name_leaves);

                        let mut j = i + 2;
                        while j < tree.len()
                            && !is_orphan_boundary(&tree[j])
                            && is_name_continuation(&tree[j])
                        {
                            let leaves = detector::token_utils::collect_all_leaves(&tree[j]);
                            cr_tokens.extend(leaves.iter());
                            holder_tokens.extend(leaves);
                            j += 1;
                        }
                        let cr_tokens = detector::token_utils::strip_all_rights_reserved(cr_tokens);
                        if let Some(det) =
                            detector::token_utils::build_copyright_from_tokens(&cr_tokens)
                        {
                            copyrights.push(det);
                        }

                        let holder_tokens =
                            detector::token_utils::strip_all_rights_reserved(holder_tokens);
                        if let Some(det) = detector::token_utils::build_holder_from_tokens(
                            &holder_tokens,
                            allow_single_word_contributors,
                        ) {
                            holders.push(det);
                        }

                        i = j;
                        continue;
                    }
                }

                let trailing_yr = get_trailing_year_range(node, tree, i + 1);

                if let Some((yr_tokens, yr_skip)) = trailing_yr {
                    let mut cr_tokens: Vec<&Token> = Vec::new();
                    if let Some(prefix) = prefix_token {
                        cr_tokens.push(prefix);
                    }
                    if let Some(prefix) = portions_prefix {
                        cr_tokens.push(prefix);
                    }
                    if let Some(prefix) = mpl_prefix.as_ref() {
                        cr_tokens.extend(prefix.iter().copied());
                    }
                    let node_leaves = detector::token_utils::collect_filtered_leaves(
                        node,
                        detector::NON_COPYRIGHT_LABELS,
                        detector::NON_COPYRIGHT_POS_TAGS,
                    );
                    let node_leaves = detector::token_utils::strip_all_rights_reserved(node_leaves);
                    cr_tokens.extend(&node_leaves);
                    cr_tokens.extend(&yr_tokens);
                    let cr_tokens = detector::token_utils::strip_all_rights_reserved(cr_tokens);
                    if let Some(det) =
                        detector::token_utils::build_copyright_from_tokens(&cr_tokens)
                    {
                        copyrights.push(det);
                    }
                    let holder = detector::token_utils::build_holder_from_node(
                        node,
                        detector::NON_HOLDER_LABELS,
                        detector::NON_HOLDER_POS_TAGS,
                    );
                    if let Some(det) = holder {
                        holders.push(det);
                    } else if let Some(det) = detector::token_utils::build_holder_from_node(
                        node,
                        detector::NON_HOLDER_LABELS_MINI,
                        detector::NON_HOLDER_POS_TAGS_MINI,
                    ) {
                        holders.push(det);
                    }
                    i += yr_skip;
                } else {
                    let mut prefixes: Vec<&Token> =
                        preceding_year_only_prefix.take().unwrap_or_default();
                    if let Some(not) = not_prefix {
                        prefixes.push(not);
                    }
                    if let Some(prefix) = portions_prefix {
                        prefixes.push(prefix);
                    }
                    if let Some(prefix) = prefix_token {
                        prefixes.push(prefix);
                    }
                    if let Some(prefix) = mpl_prefix.as_ref() {
                        prefixes.extend(prefix.iter().copied());
                    }

                    let cr_ok = if let Some(det) = {
                        let leaves = detector::token_utils::collect_filtered_leaves(
                            node,
                            detector::NON_COPYRIGHT_LABELS,
                            detector::NON_COPYRIGHT_POS_TAGS,
                        );
                        let filtered = detector::token_utils::strip_all_rights_reserved(leaves);
                        let mut all_tokens: Vec<&Token> = Vec::new();
                        all_tokens.extend(&prefixes);
                        all_tokens.extend(filtered);
                        detector::token_utils::build_copyright_from_tokens(&all_tokens)
                    } {
                        copyrights.push(det);
                        true
                    } else {
                        false
                    };

                    if let Some(not) = not_prefix {
                        let mut holder_tokens: Vec<&Token> = vec![not];
                        let node_holder_leaves =
                            detector::token_utils::collect_holder_filtered_leaves(
                                node,
                                detector::NON_HOLDER_LABELS,
                                detector::NON_HOLDER_POS_TAGS,
                            );
                        let node_holder_leaves =
                            detector::token_utils::strip_all_rights_reserved(node_holder_leaves);
                        holder_tokens.extend(node_holder_leaves);
                        let holder_tokens =
                            detector::token_utils::strip_all_rights_reserved(holder_tokens);
                        if let Some(det) = detector::token_utils::build_holder_from_tokens(
                            &holder_tokens,
                            allow_single_word_contributors,
                        ) {
                            holders.push(det);
                        }
                    } else {
                        let holder = detector::token_utils::build_holder_from_copyright_node(
                            node,
                            detector::NON_HOLDER_LABELS,
                            detector::NON_HOLDER_POS_TAGS,
                        );
                        if let Some(det) = holder {
                            holders.push(det);
                        } else if let Some(det) =
                            detector::token_utils::build_holder_from_copyright_node(
                                node,
                                detector::NON_HOLDER_LABELS_MINI,
                                detector::NON_HOLDER_POS_TAGS_MINI,
                            )
                        {
                            holders.push(det);
                        }
                    }
                    if cr_ok
                        && let Some(det) = super::author::extract_author_from_copyright_node(node)
                    {
                        authors.push(det);
                    }
                }
            } else {
                let mut cr_tokens: Vec<&Token> = Vec::new();
                if let Some(prefix) = prefix_token {
                    cr_tokens.push(prefix);
                }
                if let Some(prefix) = portions_prefix {
                    cr_tokens.push(prefix);
                }
                if let Some(prefix) = mpl_prefix.as_ref() {
                    cr_tokens.extend(prefix.iter().copied());
                }
                let node_leaves = detector::token_utils::collect_filtered_leaves(
                    node,
                    detector::NON_COPYRIGHT_LABELS,
                    detector::NON_COPYRIGHT_POS_TAGS,
                );
                let node_leaves = detector::token_utils::strip_all_rights_reserved(node_leaves);
                cr_tokens.extend(&node_leaves);

                let mut short_cr_tokens = cr_tokens.clone();

                let copy_count = detector::token_utils::collect_all_leaves(node)
                    .iter()
                    .filter(|t| t.tag == PosTag::Copy)
                    .count();
                let emit_short_linux_variant = copy_count == 1
                    && trailing_tokens
                        .first()
                        .is_some_and(|t| t.tag == PosTag::Linux);

                cr_tokens.extend(&trailing_tokens);
                cr_tokens.extend(&trailing_copyright_only_tokens);

                let cr_tokens = detector::token_utils::strip_all_rights_reserved(cr_tokens);
                short_cr_tokens = detector::token_utils::strip_all_rights_reserved(short_cr_tokens);
                let full_cr = detector::token_utils::build_copyright_from_tokens(&cr_tokens);
                if let Some(det) = full_cr.as_ref() {
                    copyrights.push(det.clone());
                }
                if emit_short_linux_variant
                    && let Some(short_det) =
                        detector::token_utils::build_copyright_from_tokens(&short_cr_tokens)
                    && full_cr
                        .as_ref()
                        .is_none_or(|f| f.copyright != short_det.copyright)
                {
                    copyrights.push(short_det);
                }

                let mut holder_tokens: Vec<&Token> = Vec::new();
                let copy_line = detector::token_utils::collect_all_leaves(node)
                    .iter()
                    .filter(|t| t.tag == PosTag::Copy && t.value.eq_ignore_ascii_case("copyright"))
                    .map(|t| t.start_line)
                    .min();
                let keep_prefix_lines = copy_line
                    .map(|cl| detector::token_utils::signal_lines_before_copy_line(node, cl))
                    .unwrap_or_default();
                let node_holder_leaves = detector::token_utils::collect_holder_filtered_leaves(
                    node,
                    detector::NON_HOLDER_LABELS,
                    detector::NON_HOLDER_POS_TAGS,
                );
                let mut node_holder_leaves =
                    detector::token_utils::strip_all_rights_reserved(node_holder_leaves);
                if let Some(copy_line) = copy_line {
                    node_holder_leaves.retain(|t| {
                        t.start_line >= copy_line || keep_prefix_lines.contains(&t.start_line.get())
                    });
                }
                detector::token_utils::strip_trailing_commas(&mut node_holder_leaves);
                holder_tokens.extend(&node_holder_leaves);

                let mut short_holder_tokens = holder_tokens.clone();

                let node_ends_with_year = {
                    let all_leaves = detector::token_utils::collect_all_leaves(node);
                    let mut found = false;
                    for t in all_leaves.iter().rev() {
                        if t.tag == PosTag::Cc && t.value == "," {
                            continue;
                        }
                        if detector::token_utils::YEAR_LIKE_POS_TAGS.contains(&t.tag) {
                            found = true;
                        }
                        break;
                    }
                    found
                };
                holder_tokens.extend(detector::token_utils::filter_holder_tokens_with_state(
                    &trailing_tokens,
                    detector::NON_HOLDER_POS_TAGS,
                    node_ends_with_year,
                ));
                let holder_tokens = detector::token_utils::strip_all_rights_reserved(holder_tokens);

                let full_holder = if let Some(det) = detector::token_utils::build_holder_from_tokens(
                    &holder_tokens,
                    allow_single_word_contributors,
                ) {
                    Some(det)
                } else {
                    let mut holder_tokens_mini: Vec<&Token> = Vec::new();
                    let node_holder_mini = detector::token_utils::collect_holder_filtered_leaves(
                        node,
                        detector::NON_HOLDER_LABELS_MINI,
                        detector::NON_HOLDER_POS_TAGS_MINI,
                    );
                    let mut node_holder_mini =
                        detector::token_utils::strip_all_rights_reserved(node_holder_mini);
                    if let Some(copy_line) = copy_line {
                        node_holder_mini.retain(|t| {
                            t.start_line >= copy_line
                                || keep_prefix_lines.contains(&t.start_line.get())
                        });
                    }
                    detector::token_utils::strip_trailing_commas(&mut node_holder_mini);
                    holder_tokens_mini.extend(&node_holder_mini);
                    let node_ends_with_year_mini = detector::token_utils::collect_all_leaves(node)
                        .last()
                        .is_some_and(|t| {
                            detector::token_utils::YEAR_LIKE_POS_TAGS.contains(&t.tag)
                        });
                    holder_tokens_mini.extend(
                        detector::token_utils::filter_holder_tokens_with_state(
                            &trailing_tokens,
                            detector::NON_HOLDER_POS_TAGS_MINI,
                            node_ends_with_year_mini,
                        ),
                    );
                    let holder_tokens_mini =
                        detector::token_utils::strip_all_rights_reserved(holder_tokens_mini);
                    detector::token_utils::build_holder_from_tokens(
                        &holder_tokens_mini,
                        allow_single_word_contributors,
                    )
                };

                if let Some(det) = full_holder.as_ref() {
                    holders.push(det.clone());
                }

                if emit_short_linux_variant {
                    short_holder_tokens =
                        detector::token_utils::strip_all_rights_reserved(short_holder_tokens);
                    if let Some(short_det) = detector::token_utils::build_holder_from_tokens(
                        &short_holder_tokens,
                        allow_single_word_contributors,
                    ) && full_holder
                        .as_ref()
                        .is_none_or(|f| f.holder != short_det.holder)
                    {
                        holders.push(short_det);
                    }
                }
                i += skip;
            }
        } else if label == Some(TreeLabel::Author) {
            if let Some(dets) = super::author::extract_sectioned_authors_from_author_node(node) {
                authors.extend(dets);
                i += 1;
                continue;
            }
            if let Some((det, skip)) = super::author::build_author_with_trailing(node, tree, i + 1)
            {
                authors.push(det);
                i += skip;
            } else if let Some(det) = detector::token_utils::build_author_from_node(node) {
                authors.push(det);
            }
        } else if let ParseNode::Leaf(token) = node
            && token.tag == PosTag::Copy
        {
            let (name_node_idx, extra_copy_tokens) =
                if i + 1 < tree.len() && is_orphan_copy_name_match(&tree[i + 1]) {
                    (Some(i + 1), vec![])
                } else if i + 2 < tree.len()
                    && matches!(&tree[i + 1], ParseNode::Leaf(t) if t.tag == PosTag::Copy)
                    && is_orphan_copy_name_match(&tree[i + 2])
                {
                    let extra = if let ParseNode::Leaf(t) = &tree[i + 1] {
                        vec![t]
                    } else {
                        vec![]
                    };
                    (Some(i + 2), extra)
                } else {
                    (None, vec![])
                };

            if let Some(name_idx) = name_node_idx {
                let next = &tree[name_idx];
                let mut cr_tokens: Vec<&Token> = Vec::new();
                if let Some(prefix) = get_orphaned_copy_prefix(tree, i) {
                    cr_tokens.push(prefix);
                }
                if i > 0
                    && let ParseNode::Leaf(prev) = &tree[i - 1]
                    && prev.tag == PosTag::Portions
                    && prev.start_line == token.start_line
                {
                    cr_tokens.push(prev);
                }
                cr_tokens.push(token);
                cr_tokens.extend(extra_copy_tokens);
                let name_leaves = detector::token_utils::collect_filtered_leaves(
                    next,
                    detector::NON_COPYRIGHT_LABELS,
                    detector::NON_COPYRIGHT_POS_TAGS,
                );
                let name_leaves = detector::token_utils::strip_all_rights_reserved(name_leaves);
                cr_tokens.extend(&name_leaves);
                let allow_single_word_contributors = cr_tokens
                    .iter()
                    .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr));
                if let Some(det) = detector::token_utils::build_copyright_from_tokens(&cr_tokens) {
                    copyrights.push(det);
                }

                let holder_leaves = detector::token_utils::collect_holder_filtered_leaves(
                    next,
                    detector::NON_HOLDER_LABELS,
                    detector::NON_HOLDER_POS_TAGS,
                );
                let holder_leaves = detector::token_utils::strip_all_rights_reserved(holder_leaves);
                if let Some(det) = detector::token_utils::build_holder_from_tokens(
                    &holder_leaves,
                    allow_single_word_contributors,
                ) {
                    holders.push(det);
                } else {
                    let holder_mini = detector::token_utils::collect_holder_filtered_leaves(
                        next,
                        detector::NON_HOLDER_LABELS_MINI,
                        detector::NON_HOLDER_POS_TAGS_MINI,
                    );
                    let holder_mini = detector::token_utils::strip_all_rights_reserved(holder_mini);
                    if let Some(det) = detector::token_utils::build_holder_from_tokens(
                        &holder_mini,
                        allow_single_word_contributors,
                    ) {
                        holders.push(det);
                    }
                }
                i = name_idx + 1;
                continue;
            }
        } else if let Some((det, skip)) = super::author::try_extract_orphaned_by_author(tree, i) {
            authors.push(det);
            i += skip;
        } else if let Some((det, skip)) = super::author::try_extract_date_by_author(tree, i) {
            authors.push(det);
            i += skip;
        } else if !group_has_copyright
            && let Some((det, skip)) = super::author::try_extract_by_name_email_author(tree, i)
        {
            authors.push(det);
            i += skip;
        }
        i += 1;
    }

    (copyrights, holders, authors)
}

fn merge_copyright_with_following_author<'a>(
    copyright_node: &'a ParseNode,
    prefix_token: Option<&'a Token>,
    tree: &'a [ParseNode],
    author_idx: usize,
) -> Option<(CopyrightDetection, Option<HolderDetection>, usize)> {
    let author_node = &tree[author_idx];
    if author_node.label() != Some(TreeLabel::Author) {
        return None;
    }

    let author_leaves = detector::token_utils::collect_all_leaves(author_node);

    let auth_token = author_leaves
        .iter()
        .find(|t| matches!(t.tag, PosTag::Auth | PosTag::AuthDot))?;
    if auth_token.tag != PosTag::Auth {
        return None;
    }

    let cr_leaves_all = detector::token_utils::collect_all_leaves(copyright_node);
    let cr_last_line = cr_leaves_all
        .last()
        .map(|t| t.start_line)
        .unwrap_or(LineNumber::ONE);
    let author_first_line = auth_token.start_line;
    if author_first_line != cr_last_line + 1 {
        return None;
    }

    let mut author_tail: Vec<&Token> = Vec::new();
    author_tail.push(auth_token);
    for t in author_leaves.iter() {
        if t.start_line < author_first_line {
            continue;
        }
        if t.start_line == author_first_line {
            continue;
        }
        if matches!(
            t.tag,
            PosTag::Email
                | PosTag::EmailStart
                | PosTag::EmailEnd
                | PosTag::Url
                | PosTag::Url2
                | PosTag::At
                | PosTag::Dot
        ) {
            continue;
        }
        if matches!(
            t.tag,
            PosTag::Nnp
                | PosTag::Nn
                | PosTag::Caps
                | PosTag::Pn
                | PosTag::MixedCap
                | PosTag::Comp
                | PosTag::Uni
                | PosTag::Van
                | PosTag::Cc
        ) {
            author_tail.push(t);
        }
    }

    if author_tail.len() < 2 {
        return None;
    }

    let mut cr_tokens: Vec<&Token> = Vec::new();
    if let Some(prefix) = prefix_token {
        cr_tokens.push(prefix);
    }
    let cr_leaves = detector::token_utils::collect_filtered_leaves(
        copyright_node,
        detector::NON_COPYRIGHT_LABELS,
        detector::NON_COPYRIGHT_POS_TAGS,
    );
    let cr_leaves = detector::token_utils::strip_all_rights_reserved(cr_leaves);
    cr_tokens.extend(&cr_leaves);

    cr_tokens.extend(author_tail);

    let cr_det = detector::token_utils::build_copyright_from_tokens(&cr_tokens)?;

    Some((cr_det, None, 0))
}

fn get_orphaned_copy_prefix(tree: &[ParseNode], idx: usize) -> Option<&Token> {
    if idx == 0 {
        return None;
    }
    let prev = &tree[idx - 1];
    if let ParseNode::Leaf(token) = prev
        && token.tag == PosTag::Copy
    {
        return Some(token);
    }
    if let ParseNode::Tree { label, children } = prev {
        match label {
            TreeLabel::NameCopy => {
                for child in children.iter().rev() {
                    if let ParseNode::Leaf(token) = child
                        && token.tag == PosTag::Copy
                    {
                        return Some(token);
                    }
                }
            }
            TreeLabel::Copyright | TreeLabel::Copyright2 => {
                let all_copy = children.iter().all(|c| {
                    matches!(c, ParseNode::Leaf(t) if t.tag == PosTag::Copy)
                        || matches!(c, ParseNode::Tree { label: l, .. }
                            if matches!(l, TreeLabel::Copyright | TreeLabel::Copyright2)
                                && is_copy_only_tree(c))
                });
                if all_copy {
                    for child in children.iter().rev() {
                        if let ParseNode::Leaf(token) = child
                            && token.tag == PosTag::Copy
                        {
                            return Some(token);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    None
}

fn get_orphaned_not_prefix<'a>(
    tree: &'a [ParseNode],
    idx: usize,
    copyright_node: &ParseNode,
    allow_not_copyrighted_prefix: bool,
) -> Option<&'a Token> {
    if !allow_not_copyrighted_prefix {
        return None;
    }
    if idx == 0 {
        return None;
    }
    let first_line = detector::token_utils::collect_all_leaves(copyright_node)
        .first()
        .map(|t| t.start_line)?;
    let prev = &tree[idx - 1];
    if let ParseNode::Leaf(token) = prev
        && token.start_line == first_line
        && token.value.eq_ignore_ascii_case("not")
    {
        for n in &tree[..idx - 1] {
            for t in detector::token_utils::collect_all_leaves(n) {
                if t.start_line != first_line {
                    continue;
                }
                if matches!(t.tag, PosTag::Junk | PosTag::Dash | PosTag::Parens)
                    || looks_like_filename_prefix_token(t)
                {
                    continue;
                }
                return None;
            }
        }
        return Some(token);
    }
    None
}

fn looks_like_filename_prefix_token(token: &Token) -> bool {
    let v = token.value.as_str();
    if v == "--" {
        return true;
    }
    if !v.contains('.') {
        return false;
    }
    let (base, ext) = match v.rsplit_once('.') {
        Some(parts) => parts,
        None => return false,
    };
    if base.is_empty()
        || ext.is_empty()
        || ext.len() > 4
        || !ext.chars().all(|c| c.is_ascii_alphabetic())
    {
        return false;
    }
    v.chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-' | '+'))
}

fn get_trailing_year_range<'a>(
    copyright_node: &ParseNode,
    tree: &'a [ParseNode],
    start: usize,
) -> Option<(Vec<&'a Token>, usize)> {
    if start >= tree.len() {
        return None;
    }
    let next = &tree[start];
    let is_yr_tree = matches!(
        next.label(),
        Some(TreeLabel::YrRange) | Some(TreeLabel::YrAnd)
    );
    if !is_yr_tree {
        return None;
    }
    let node_has_year = detector::token_utils::collect_all_leaves(copyright_node)
        .iter()
        .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr));
    if node_has_year {
        return None;
    }
    let yr_tokens = detector::token_utils::collect_all_leaves(next);
    Some((yr_tokens, 1))
}

fn is_copy_only_tree(node: &ParseNode) -> bool {
    match node {
        ParseNode::Leaf(t) => t.tag == PosTag::Copy,
        ParseNode::Tree { children, .. } => children.iter().all(is_copy_only_tree),
    }
}

fn is_orphan_continuation(node: &ParseNode) -> bool {
    match node {
        ParseNode::Leaf(token) => matches!(
            token.tag,
            PosTag::Of
                | PosTag::Van
                | PosTag::Uni
                | PosTag::Yr
                | PosTag::YrPlus
                | PosTag::BareYr
                | PosTag::Nn
                | PosTag::Nnp
                | PosTag::Caps
                | PosTag::Cc
                | PosTag::Cd
                | PosTag::Cds
                | PosTag::Comp
                | PosTag::Dash
                | PosTag::Pn
                | PosTag::MixedCap
                | PosTag::In
                | PosTag::To
                | PosTag::By
                | PosTag::Oth
                | PosTag::Email
                | PosTag::Url
                | PosTag::Url2
                | PosTag::Linux
                | PosTag::Parens
        ),
        ParseNode::Tree { label, .. } => matches!(
            label,
            TreeLabel::Name
                | TreeLabel::NameEmail
                | TreeLabel::NameYear
                | TreeLabel::NameCaps
                | TreeLabel::Company
                | TreeLabel::AndCo
                | TreeLabel::YrRange
                | TreeLabel::YrAnd
                | TreeLabel::DashCaps
        ),
    }
}

fn is_name_continuation(node: &ParseNode) -> bool {
    match node {
        ParseNode::Leaf(token) => matches!(
            token.tag,
            PosTag::Nnp
                | PosTag::Caps
                | PosTag::Comp
                | PosTag::MixedCap
                | PosTag::Cc
                | PosTag::Dash
                | PosTag::Of
                | PosTag::Van
                | PosTag::Linux
                | PosTag::Email
                | PosTag::Url
                | PosTag::Url2
        ),
        ParseNode::Tree { label, .. } => matches!(
            label,
            TreeLabel::Name
                | TreeLabel::NameEmail
                | TreeLabel::NameCaps
                | TreeLabel::NameYear
                | TreeLabel::Company
                | TreeLabel::AndCo
                | TreeLabel::DashCaps
        ),
    }
}

fn is_same_line_holder_suffix_prefix(tree: &[ParseNode], idx: usize, line: LineNumber) -> bool {
    let Some(node) = tree.get(idx) else {
        return false;
    };
    let leaves = detector::token_utils::collect_all_leaves(node);
    let Some(first_token) = leaves.first() else {
        return false;
    };
    if first_token.start_line != line {
        return false;
    }

    let is_name_like_prefix = matches!(
        first_token.tag,
        PosTag::Nnp
            | PosTag::Nn
            | PosTag::Caps
            | PosTag::Comp
            | PosTag::MixedCap
            | PosTag::Uni
            | PosTag::Pn
            | PosTag::Ou
            | PosTag::Of
            | PosTag::Van
    );
    if !is_name_like_prefix {
        return false;
    }

    let end = std::cmp::min(idx + 6, tree.len());
    tree[idx..end].iter().any(|node| {
        detector::token_utils::collect_all_leaves(node)
            .iter()
            .any(|token| {
                token.start_line == line
                    && matches!(
                        token.tag,
                        PosTag::Auths | PosTag::AuthDot | PosTag::Contributors | PosTag::Commit
                    )
            })
    })
}

fn has_same_line_confidential_proprietary_suffix(
    copyright_node: &ParseNode,
    tree: &[ParseNode],
    start: usize,
    line: LineNumber,
) -> bool {
    let node_has_confidential = detector::token_utils::collect_all_leaves(copyright_node)
        .iter()
        .any(|t| t.start_line == line && t.value.eq_ignore_ascii_case("Confidential"));
    if !node_has_confidential {
        return false;
    }

    let end = std::cmp::min(start + 6, tree.len());
    tree[start + 1..end].iter().any(|node| {
        detector::token_utils::collect_all_leaves(node)
            .iter()
            .any(|token| {
                token.start_line == line
                    && token
                        .value
                        .trim_end_matches(|c: char| c.is_ascii_punctuation())
                        .eq_ignore_ascii_case("proprietary")
            })
    })
}

fn is_orphan_copy_name_match(node: &ParseNode) -> bool {
    match node.label() {
        Some(TreeLabel::NameYear) | Some(TreeLabel::NameEmail) | Some(TreeLabel::Company) => true,
        Some(TreeLabel::Name | TreeLabel::NameCaps) => {
            let leaves = detector::token_utils::collect_all_leaves(node);
            leaves
                .iter()
                .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr))
        }
        _ => false,
    }
}

fn is_orphan_boundary(node: &ParseNode) -> bool {
    match node {
        ParseNode::Leaf(token) => matches!(
            token.tag,
            PosTag::EmptyLine
                | PosTag::Copy
                | PosTag::Auth
                | PosTag::Auth2
                | PosTag::Auths
                | PosTag::AuthDot
                | PosTag::Maint
                | PosTag::Contributors
                | PosTag::Commit
                | PosTag::SpdxContrib
                | PosTag::Junk
        ),
        ParseNode::Tree { label, .. } => matches!(
            label,
            TreeLabel::Copyright
                | TreeLabel::Copyright2
                | TreeLabel::Author
                | TreeLabel::AllRightReserved
        ),
    }
}

pub fn should_start_absorbing(
    copyright_node: &ParseNode,
    tree: &[ParseNode],
    start: usize,
) -> bool {
    if start >= tree.len() {
        return false;
    }
    let first = &tree[start];

    let last_line = detector::token_utils::collect_all_leaves(copyright_node)
        .last()
        .map(|t| t.start_line);

    if last_line.is_some()
        && last_line
            == detector::token_utils::collect_all_leaves(first)
                .first()
                .map(|t| t.start_line)
    {
        let last_tag = detector::token_utils::collect_all_leaves(copyright_node)
            .last()
            .map(|t| t.tag);
        if matches!(
            last_tag,
            Some(PosTag::Auths)
                | Some(PosTag::AuthDot)
                | Some(PosTag::Contributors)
                | Some(PosTag::Commit)
        ) {
            if is_orphan_continuation(first) {
                return true;
            }
            if let ParseNode::Leaf(token) = first
                && token.value.eq_ignore_ascii_case("as")
            {
                return true;
            }
        }
    }

    if let ParseNode::Leaf(token) = first
        && matches!(
            token.tag,
            PosTag::Auths | PosTag::AuthDot | PosTag::Contributors | PosTag::Commit
        )
    {
        let same_line = last_line.is_some_and(|l| l == token.start_line);
        let node_has_year = detector::token_utils::collect_all_leaves(copyright_node)
            .iter()
            .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr));
        let has_holder_like_tokens = detector::token_utils::collect_all_leaves(copyright_node)
            .iter()
            .any(|t| {
                matches!(
                    t.tag,
                    PosTag::Nnp
                        | PosTag::Caps
                        | PosTag::Comp
                        | PosTag::MixedCap
                        | PosTag::Uni
                        | PosTag::Pn
                        | PosTag::Ou
                        | PosTag::Url
                        | PosTag::Url2
                        | PosTag::Email
                )
            });
        if same_line && (has_holder_like_tokens || node_has_year) {
            return true;
        }
    }

    if let ParseNode::Tree {
        label: TreeLabel::Author | TreeLabel::AndAuth,
        ..
    } = first
    {
        let leaves = detector::token_utils::collect_all_leaves(first);
        let same_line =
            !leaves.is_empty() && leaves.iter().all(|t| last_line == Some(t.start_line));
        let has_author_keyword = leaves.iter().any(|t| {
            matches!(
                t.tag,
                PosTag::Auths | PosTag::AuthDot | PosTag::Contributors | PosTag::Commit
            )
        });
        if same_line && has_author_keyword {
            let node_has_year = detector::token_utils::collect_all_leaves(copyright_node)
                .iter()
                .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr));
            if node_has_year {
                return true;
            }
        }
    }

    if let ParseNode::Leaf(token) = first
        && token.tag == PosTag::Uni
        && last_line.is_some_and(|l| l == token.start_line)
    {
        return true;
    }

    if let ParseNode::Leaf(token) = first
        && token.tag == PosTag::By
        && last_line.is_some_and(|l| l == token.start_line)
    {
        let node_has_holder = detector::token_utils::build_holder_from_node(
            copyright_node,
            detector::NON_HOLDER_LABELS,
            detector::NON_HOLDER_POS_TAGS,
        )
        .is_some();
        if !node_has_holder && has_name_like_within(tree, start + 1, 3) {
            return true;
        }
    }

    if let ParseNode::Leaf(token) = first
        && token.tag == PosTag::Cd
        && last_line.is_some_and(|l| l == token.start_line)
    {
        let end = std::cmp::min(start + 5, tree.len());
        let has_company_suffix = tree[start..end].iter().any(|n| {
            detector::token_utils::collect_all_leaves(n)
                .iter()
                .any(|t| t.tag == PosTag::Comp)
        });
        let has_comma_boundary = token.value.ends_with(',')
            || tree.get(start + 1).is_some_and(|n| {
                detector::token_utils::collect_all_leaves(n)
                    .iter()
                    .any(|t| t.value == ",")
            });
        if has_company_suffix && has_comma_boundary {
            return true;
        }
    }

    if let ParseNode::Leaf(token) = first
        && token.tag == PosTag::Dash
        && last_line.is_some_and(|l| l == token.start_line)
    {
        let end = std::cmp::min(start + 5, tree.len());
        let has_email = tree[start..end].iter().any(|n| {
            detector::token_utils::collect_all_leaves(n)
                .iter()
                .any(|t| t.tag == PosTag::Email)
        });
        if has_email {
            return true;
        }
    }

    if let ParseNode::Leaf(token) = first
        && token.value.eq_ignore_ascii_case("as")
    {
        let end = std::cmp::min(start + 8, tree.len());
        let has_expected_title = tree[start..end].iter().any(|n| {
            detector::token_utils::collect_all_leaves(n)
                .iter()
                .any(|t| {
                    t.value.eq_ignore_ascii_case("secretary")
                        || t.value.eq_ignore_ascii_case("administrator")
                })
        });
        if has_expected_title {
            let same_line = last_line.is_some_and(|l| l == token.start_line);
            let has_holder_like_tokens = detector::token_utils::collect_all_leaves(copyright_node)
                .iter()
                .any(|t| {
                    matches!(
                        t.tag,
                        PosTag::Nnp
                            | PosTag::Caps
                            | PosTag::Comp
                            | PosTag::MixedCap
                            | PosTag::Uni
                            | PosTag::Pn
                            | PosTag::Ou
                    )
                });
            if same_line && has_holder_like_tokens {
                return true;
            }
        }
    }

    if is_year_only_copyright_clause_node(copyright_node)
        && let ParseNode::Leaf(token) = first
        && token.tag == PosTag::Nn
        && last_line.is_some_and(|l| l == token.start_line)
        && token.value == "Name"
    {
        return true;
    }

    if let ParseNode::Leaf(token) = first
        && last_line.is_some_and(|l| l == token.start_line)
        && matches!(
            token.tag,
            PosTag::Nnp
                | PosTag::Nn
                | PosTag::Caps
                | PosTag::Comp
                | PosTag::MixedCap
                | PosTag::Uni
                | PosTag::Pn
                | PosTag::Ou
                | PosTag::Url
                | PosTag::Url2
        )
    {
        let end = std::cmp::min(start + 6, tree.len());
        let suffix_boundary_on_same_line = tree[start..end].iter().any(|n| {
            detector::token_utils::collect_all_leaves(n)
                .iter()
                .any(|t| {
                    t.start_line == token.start_line
                        && matches!(
                            t.tag,
                            PosTag::Auths | PosTag::AuthDot | PosTag::Contributors | PosTag::Commit
                        )
                })
        });
        if suffix_boundary_on_same_line {
            return true;
        }
    }

    if let ParseNode::Leaf(token) = first
        && last_line.is_some_and(|l| l == token.start_line)
        && (token.value == "," || token.tag == PosTag::Cc)
    {
        let end = std::cmp::min(start + 6, tree.len());
        let has_expected_continuation = tree[start + 1..end].iter().any(|n| {
            is_name_continuation(n)
                || matches!(n.label(), Some(TreeLabel::YrRange) | Some(TreeLabel::YrAnd))
                || detector::token_utils::collect_all_leaves(n)
                    .iter()
                    .any(|t| t.tag == PosTag::Maint)
        });
        let has_holder_suffix_prefix =
            tree[start + 1..end].iter().enumerate().any(|(offset, _)| {
                is_same_line_holder_suffix_prefix(tree, start + 1 + offset, token.start_line)
            });
        let has_confidential_proprietary_suffix = has_same_line_confidential_proprietary_suffix(
            copyright_node,
            tree,
            start,
            token.start_line,
        );
        if has_expected_continuation
            || has_holder_suffix_prefix
            || has_confidential_proprietary_suffix
        {
            return true;
        }
    }

    if copyright_node.label() == Some(TreeLabel::Copyright2)
        && let ParseNode::Tree {
            label: TreeLabel::NameCaps,
            ..
        } = first
    {
        let leaves = detector::token_utils::collect_all_leaves(first);
        let same_line = !leaves.is_empty()
            && last_line.is_some_and(|l| leaves.first().is_some_and(|t| t.start_line == l));
        let node_has_year = detector::token_utils::collect_all_leaves(copyright_node)
            .iter()
            .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr));
        let last_tag = detector::token_utils::collect_all_leaves(copyright_node)
            .last()
            .map(|t| t.tag);
        if same_line && node_has_year && matches!(last_tag, Some(PosTag::Caps)) {
            return true;
        }
    }

    let strong_first = match first {
        ParseNode::Leaf(token) if token.tag == PosTag::Of || token.tag == PosTag::Van => {
            has_name_like_within(tree, start + 1, 2)
        }
        ParseNode::Tree { label, .. } => matches!(
            label,
            TreeLabel::Name
                | TreeLabel::NameEmail
                | TreeLabel::NameYear
                | TreeLabel::Company
                | TreeLabel::AndCo
                | TreeLabel::NameCaps
                | TreeLabel::DashCaps
        ),
        _ => false,
    };

    if strong_first {
        return true;
    }

    if last_leaf_ends_with_comma(copyright_node) {
        let node_has_year = detector::token_utils::collect_all_leaves(copyright_node)
            .iter()
            .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr));
        if node_has_year {
            let is_name_like_first = match first {
                ParseNode::Leaf(token) => matches!(
                    token.tag,
                    PosTag::Nnp | PosTag::Caps | PosTag::Comp | PosTag::Uni | PosTag::MixedCap
                ),
                _ => false,
            };
            if is_name_like_first {
                return has_company_signal_nearby(tree, start);
            }
        }
    }

    let is_name_like_first = match first {
        ParseNode::Leaf(token) => matches!(
            token.tag,
            PosTag::Nnp | PosTag::Caps | PosTag::Cd | PosTag::Cds | PosTag::Comp | PosTag::MixedCap
        ),
        _ => false,
    };
    if is_name_like_first {
        return has_company_signal_nearby(tree, start);
    }

    if let ParseNode::Leaf(token) = first
        && token.tag == PosTag::Linux
        && last_line.is_some_and(|l| l == token.start_line)
        && has_company_signal_nearby(tree, start)
    {
        let copy_count = detector::token_utils::collect_all_leaves(copyright_node)
            .iter()
            .filter(|t| t.tag == PosTag::Copy)
            .count();
        if copy_count != 1 {
            return false;
        }
        let has_holder_like_tokens = detector::token_utils::collect_all_leaves(copyright_node)
            .iter()
            .any(|t| {
                matches!(
                    t.tag,
                    PosTag::Nnp
                        | PosTag::Caps
                        | PosTag::Comp
                        | PosTag::MixedCap
                        | PosTag::Uni
                        | PosTag::Pn
                        | PosTag::Ou
                        | PosTag::Url
                        | PosTag::Url2
                        | PosTag::Email
                )
            });
        if has_holder_like_tokens {
            return true;
        }
    }

    false
}

fn has_name_tree_within(tree: &[ParseNode], start: usize, lookahead: usize) -> bool {
    let end = std::cmp::min(start + lookahead, tree.len());
    for node in &tree[start..end] {
        if let ParseNode::Tree { label, .. } = node
            && matches!(
                label,
                TreeLabel::Name | TreeLabel::Company | TreeLabel::NameEmail
            )
        {
            return true;
        }
    }
    false
}

fn has_name_like_within(tree: &[ParseNode], start: usize, lookahead: usize) -> bool {
    let end = std::cmp::min(start + lookahead, tree.len());
    for node in &tree[start..end] {
        match node {
            ParseNode::Leaf(token) => {
                if matches!(
                    token.tag,
                    PosTag::Uni | PosTag::Nnp | PosTag::Caps | PosTag::Comp
                ) {
                    return true;
                }
            }
            ParseNode::Tree { label, .. } => {
                if matches!(
                    label,
                    TreeLabel::Name | TreeLabel::Company | TreeLabel::NameEmail
                ) {
                    return true;
                }
            }
        }
    }
    false
}

fn has_company_signal_nearby(tree: &[ParseNode], start: usize) -> bool {
    let end = std::cmp::min(start + 3, tree.len());
    for node in &tree[start..end] {
        match node {
            ParseNode::Leaf(token) => {
                if matches!(token.tag, PosTag::Comp) {
                    return true;
                }
            }
            ParseNode::Tree { label, .. } => {
                if matches!(label, TreeLabel::Company) {
                    return true;
                }
            }
        }
    }
    false
}

fn last_leaf_ends_with_comma(node: &ParseNode) -> bool {
    let leaves = detector::token_utils::collect_all_leaves(node);
    leaves.last().is_some_and(|t| t.value.ends_with(','))
}

pub fn collect_trailing_orphan_tokens<'a>(
    copyright_node: &'a ParseNode,
    tree: &'a [ParseNode],
    start: usize,
) -> (Vec<&'a Token>, usize) {
    if !should_start_absorbing(copyright_node, tree, start) {
        return (Vec::new(), 0);
    }

    fn is_allowed_holder_suffix_boundary_on_same_line(
        copyright_node: &ParseNode,
        node: &ParseNode,
    ) -> bool {
        let last_line = detector::token_utils::collect_all_leaves(copyright_node)
            .last()
            .map(|t| t.start_line);
        let Some(last_line) = last_line else {
            return false;
        };

        match node {
            ParseNode::Leaf(token) => {
                token.start_line == last_line
                    && matches!(
                        token.tag,
                        PosTag::Auths
                            | PosTag::AuthDot
                            | PosTag::Maint
                            | PosTag::Contributors
                            | PosTag::Commit
                    )
            }
            ParseNode::Tree {
                label: TreeLabel::Author | TreeLabel::AndAuth,
                ..
            } => {
                let leaves = detector::token_utils::collect_all_leaves(node);
                !leaves.is_empty()
                    && leaves.iter().all(|t| t.start_line == last_line)
                    && leaves.iter().any(|t| {
                        matches!(
                            t.tag,
                            PosTag::Auths
                                | PosTag::AuthDot
                                | PosTag::Maint
                                | PosTag::Contributors
                                | PosTag::Commit
                        )
                    })
            }
            _ => false,
        }
    }

    let mut tokens: Vec<&Token> = Vec::new();
    let mut j = start;

    let last_line = detector::token_utils::collect_all_leaves(copyright_node)
        .last()
        .map(|t| t.start_line);

    while j < tree.len() {
        let node = &tree[j];

        if let Some(last_line) = last_line
            && matches!(
                node.label(),
                Some(TreeLabel::Copyright) | Some(TreeLabel::Copyright2)
            )
        {
            let leaves = detector::token_utils::collect_all_leaves(node);
            if leaves.first().is_some_and(|t| t.start_line > last_line) {
                break;
            }
        }

        let allowed_suffix = is_allowed_holder_suffix_boundary_on_same_line(copyright_node, node);
        let allowed_suffix_prefix =
            last_line.is_some_and(|line| is_same_line_holder_suffix_prefix(tree, j, line));

        let allow_junk_file = match node {
            ParseNode::Leaf(token)
                if token.tag == PosTag::Junk && token.value.eq_ignore_ascii_case("file") =>
            {
                tokens
                    .last()
                    .is_some_and(|prev| prev.value.eq_ignore_ascii_case("AUTHORS"))
            }
            _ => false,
        };

        if is_orphan_boundary(node) && !allowed_suffix && !allowed_suffix_prefix && !allow_junk_file
        {
            break;
        }

        if !is_orphan_continuation(node)
            && !allowed_suffix
            && !allowed_suffix_prefix
            && !allow_junk_file
        {
            break;
        }

        let leaves = detector::token_utils::collect_all_leaves(node);
        let already_have_url = tokens
            .iter()
            .any(|t| matches!(t.tag, PosTag::Url | PosTag::Url2));
        let leaves_have_url = leaves
            .iter()
            .any(|t| matches!(t.tag, PosTag::Url | PosTag::Url2));
        if already_have_url && leaves_have_url {
            break;
        }

        tokens.extend(leaves);
        j += 1;
    }

    let skip = j - start;
    (tokens, skip)
}

fn collect_following_copyright_clause_tokens(
    tree: &[ParseNode],
    start: usize,
    line: LineNumber,
) -> (Vec<&Token>, usize) {
    if start >= tree.len() {
        return (Vec::new(), 0);
    }

    match &tree[start] {
        ParseNode::Leaf(token)
            if token.tag == PosTag::Copy && token.value.eq_ignore_ascii_case("copyright") => {}
        _ => return (Vec::new(), 0),
    }

    let mut tokens: Vec<&Token> = Vec::new();
    let mut j = start;
    let max_nodes = std::cmp::min(start + 16, tree.len());

    while j < max_nodes {
        let node = &tree[j];
        let leaves = detector::token_utils::collect_all_leaves(node);
        if leaves.first().is_none_or(|t| t.start_line != line) {
            break;
        }

        if j != start && is_orphan_boundary(node) {
            break;
        }

        tokens.extend(leaves);
        j += 1;
    }

    let skip = j - start;
    let has_year = tokens
        .iter()
        .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr));

    if !has_year {
        return (Vec::new(), 0);
    }

    let has_name_like = tokens.iter().any(|t| {
        matches!(
            t.tag,
            PosTag::Nnp
                | PosTag::Caps
                | PosTag::Comp
                | PosTag::MixedCap
                | PosTag::Uni
                | PosTag::Pn
                | PosTag::Ou
                | PosTag::Email
                | PosTag::Url
                | PosTag::Url2
        )
    });
    if has_name_like {
        return (Vec::new(), 0);
    }

    (tokens, skip)
}

fn is_year_only_copyright_clause_node(node: &ParseNode) -> bool {
    if !matches!(
        node.label(),
        Some(TreeLabel::Copyright) | Some(TreeLabel::Copyright2)
    ) {
        return false;
    }

    let leaves = detector::token_utils::collect_all_leaves(node);
    let has_year = leaves
        .iter()
        .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr));
    if !has_year {
        return false;
    }

    let has_holder = detector::token_utils::build_holder_from_node(
        node,
        detector::NON_HOLDER_LABELS,
        detector::NON_HOLDER_POS_TAGS,
    )
    .is_some()
        || detector::token_utils::build_holder_from_node(
            node,
            detector::NON_HOLDER_LABELS_MINI,
            detector::NON_HOLDER_POS_TAGS_MINI,
        )
        .is_some();
    !has_holder
}

fn merge_year_only_copyright_clause_with_preceding_copyrighted_by(
    tree: &[ParseNode],
    copyright_idx: usize,
    copy_prefix: Option<&Token>,
    portions_prefix: Option<&Token>,
    mpl_prefix: Option<&[&Token]>,
) -> Option<(CopyrightDetection, HolderDetection)> {
    if copyright_idx >= tree.len() {
        return None;
    }
    let node = &tree[copyright_idx];
    if !is_year_only_copyright_clause_node(node) {
        return None;
    }

    let node_line = detector::token_utils::collect_all_leaves(node)
        .first()
        .map(|t| t.start_line)?;

    let mut copyrighted_idx: Option<usize> = None;
    let mut by_idx: Option<usize> = None;

    let start_search = copyright_idx.saturating_sub(14);
    for idx in (start_search..copyright_idx).rev() {
        let leaves = detector::token_utils::collect_all_leaves(&tree[idx]);
        if leaves.first().is_none_or(|t| t.start_line != node_line) {
            continue;
        }
        if let ParseNode::Leaf(token) = &tree[idx]
            && token.tag == PosTag::Copy
            && token.value.eq_ignore_ascii_case("copyrighted")
        {
            copyrighted_idx = Some(idx);
            break;
        }
    }
    let copyrighted_idx = copyrighted_idx?;

    for (idx, node) in tree
        .iter()
        .enumerate()
        .take(copyright_idx)
        .skip(copyrighted_idx + 1)
    {
        let leaves = detector::token_utils::collect_all_leaves(node);
        if leaves.first().is_none_or(|t| t.start_line != node_line) {
            break;
        }
        if let ParseNode::Leaf(token) = node
            && token.tag == PosTag::By
            && token.value.eq_ignore_ascii_case("by")
        {
            by_idx = Some(idx);
            break;
        }
    }
    let by_idx = by_idx?;

    if by_idx + 1 >= copyright_idx {
        return None;
    }

    let has_comma_boundary = (by_idx + 1..copyright_idx).any(|idx| {
        detector::token_utils::collect_all_leaves(&tree[idx])
            .iter()
            .any(|t| t.value == "," || t.tag == PosTag::Cc || t.value.ends_with(','))
    });
    if !has_comma_boundary {
        return None;
    }

    let mut cr_tokens: Vec<&Token> = Vec::new();
    if let Some(prefix) = copy_prefix {
        cr_tokens.push(prefix);
    }
    if let Some(prefix) = portions_prefix {
        cr_tokens.push(prefix);
    }
    if let Some(prefix) = mpl_prefix {
        cr_tokens.extend(prefix.iter().copied());
    }

    for node in tree.iter().take(copyright_idx + 1).skip(copyrighted_idx) {
        cr_tokens.extend(detector::token_utils::collect_all_leaves(node));
    }
    let cr_tokens = detector::token_utils::strip_all_rights_reserved(cr_tokens);
    let cr_det = detector::token_utils::build_copyright_from_tokens(&cr_tokens)?;

    let mut holder_tokens: Vec<&Token> = Vec::new();
    for node in tree.iter().take(copyright_idx).skip(by_idx + 1) {
        holder_tokens.extend(detector::token_utils::collect_all_leaves(node));
    }
    let holder_tokens = detector::token_utils::strip_all_rights_reserved(holder_tokens);
    let allow_single_word_contributors = holder_tokens
        .iter()
        .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr));
    let holder_det = detector::token_utils::build_holder_from_tokens(
        &holder_tokens,
        allow_single_word_contributors,
    )?;

    Some((cr_det, holder_det))
}

pub fn extract_holder_is_name(
    tree: &[ParseNode],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    let mut copyrights: Vec<CopyrightDetection> = Vec::new();
    let mut holders: Vec<HolderDetection> = Vec::new();

    let mut i = 0;
    while i < tree.len() {
        if let ParseNode::Leaf(token) = &tree[i]
            && token.tag == PosTag::Holder
            && i + 2 < tree.len()
            && let ParseNode::Leaf(is_token) = &tree[i + 1]
            && is_token.tag == PosTag::Is
            && matches!(
                tree[i + 2].label(),
                Some(TreeLabel::Name)
                    | Some(TreeLabel::NameEmail)
                    | Some(TreeLabel::NameYear)
                    | Some(TreeLabel::NameCaps)
                    | Some(TreeLabel::Company)
            )
        {
            let name_leaves = detector::token_utils::collect_filtered_leaves(
                &tree[i + 2],
                detector::NON_COPYRIGHT_LABELS,
                detector::NON_COPYRIGHT_POS_TAGS,
            );
            let name_leaves_stripped =
                detector::token_utils::strip_all_rights_reserved(name_leaves);
            let mut cr_tokens: Vec<&Token> = vec![token, is_token];
            cr_tokens.extend(&name_leaves_stripped);
            if let Some(det) = detector::token_utils::build_copyright_from_tokens(&cr_tokens) {
                copyrights.push(det);
            }

            let holder_leaves = detector::token_utils::collect_holder_filtered_leaves(
                &tree[i + 2],
                detector::NON_HOLDER_LABELS,
                detector::NON_HOLDER_POS_TAGS,
            );
            let holder_leaves = detector::token_utils::strip_all_rights_reserved(holder_leaves);
            if let Some(det) =
                detector::token_utils::build_holder_from_tokens(&holder_leaves, false)
            {
                holders.push(det);
            }
            i += 3;
            continue;
        }
        i += 1;
    }

    (copyrights, holders)
}

pub fn extract_bare_copyrights(
    tree: &[ParseNode],
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    fn has_line_start_copyright_prefix(tree: &[ParseNode], idx: usize, line: LineNumber) -> bool {
        let mut found_copyright = false;
        for j in (0..idx).rev() {
            for t in detector::token_utils::collect_all_leaves(&tree[j])
                .iter()
                .rev()
            {
                if t.start_line != line {
                    continue;
                }
                if !found_copyright {
                    if t.tag == PosTag::Copy && t.value.eq_ignore_ascii_case("copyright") {
                        found_copyright = true;
                        continue;
                    }
                    return false;
                }
                return false;
            }
        }
        found_copyright
    }

    let mut copyrights: Vec<CopyrightDetection> = Vec::new();
    let mut holders: Vec<HolderDetection> = Vec::new();

    let mut i = 0;
    while i < tree.len() {
        if let ParseNode::Leaf(token) = &tree[i]
            && token.tag == PosTag::Copy
            && i + 1 < tree.len()
        {
            if token.value.eq_ignore_ascii_case("(c)")
                && has_line_start_copyright_prefix(tree, i, token.start_line)
            {
                i += 1;
                continue;
            }

            let next = &tree[i + 1];
            if matches!(
                next.label(),
                Some(TreeLabel::NameYear)
                    | Some(TreeLabel::Name)
                    | Some(TreeLabel::NameEmail)
                    | Some(TreeLabel::NameCaps)
                    | Some(TreeLabel::Company)
            ) {
                let portions_prefix = if i > 0
                    && let ParseNode::Leaf(prev) = &tree[i - 1]
                    && prev.tag == PosTag::Portions
                {
                    Some(prev)
                } else {
                    None
                };

                let mut cr_tokens: Vec<&Token> = Vec::new();
                if let Some(prefix) = portions_prefix {
                    cr_tokens.push(prefix);
                }
                cr_tokens.push(token);
                let name_leaves = detector::token_utils::collect_filtered_leaves(
                    next,
                    detector::NON_COPYRIGHT_LABELS,
                    detector::NON_COPYRIGHT_POS_TAGS,
                );
                let name_leaves = detector::token_utils::strip_all_rights_reserved(name_leaves);
                let allow_single_word_contributors = name_leaves
                    .iter()
                    .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr));
                cr_tokens.extend(&name_leaves);

                let mut extra_skip = 0usize;
                let mut j = i + 2;
                while j < tree.len() {
                    match &tree[j] {
                        ParseNode::Leaf(t)
                            if t.start_line == token.start_line
                                && matches!(
                                    t.tag,
                                    PosTag::Cc | PosTag::Email | PosTag::Url | PosTag::Url2
                                ) =>
                        {
                            cr_tokens.push(t);
                            j += 1;
                            extra_skip += 1;
                        }
                        _ => break,
                    }
                }
                if let Some(det) = detector::token_utils::build_copyright_from_tokens(&cr_tokens) {
                    copyrights.push(det);
                }

                let holder_leaves = detector::token_utils::collect_holder_filtered_leaves(
                    next,
                    detector::NON_HOLDER_LABELS,
                    detector::NON_HOLDER_POS_TAGS,
                );
                let holder_leaves = detector::token_utils::strip_all_rights_reserved(holder_leaves);
                if let Some(det) = detector::token_utils::build_holder_from_tokens(
                    &holder_leaves,
                    allow_single_word_contributors,
                ) {
                    holders.push(det);
                } else {
                    let holder_mini = detector::token_utils::collect_holder_filtered_leaves(
                        next,
                        detector::NON_HOLDER_LABELS_MINI,
                        detector::NON_HOLDER_POS_TAGS_MINI,
                    );
                    let holder_mini = detector::token_utils::strip_all_rights_reserved(holder_mini);
                    if let Some(det) = detector::token_utils::build_holder_from_tokens(
                        &holder_mini,
                        allow_single_word_contributors,
                    ) {
                        holders.push(det);
                    }
                }
                i += 2 + extra_skip;
                continue;
            }
        }
        i += 1;
    }

    (copyrights, holders)
}

pub fn extract_from_spans(
    tree: &[ParseNode],
    allow_not_copyrighted_prefix: bool,
) -> (
    Vec<CopyrightDetection>,
    Vec<HolderDetection>,
    Vec<AuthorDetection>,
) {
    let mut copyrights: Vec<CopyrightDetection> = Vec::new();
    let mut holders: Vec<HolderDetection> = Vec::new();
    let mut authors: Vec<AuthorDetection> = Vec::new();

    let all_leaves: Vec<&Token> = tree.iter().flat_map(detector::collect_all_leaves).collect();

    if all_leaves.is_empty() {
        return (copyrights, holders, authors);
    }

    let mut i = 0;
    while i < all_leaves.len() {
        let token = all_leaves[i];

        let has_line_start_copyright_prefix =
            if token.tag == PosTag::Copy && token.value.eq_ignore_ascii_case("(c)") {
                let line = token.start_line;
                let mut found_copyright = false;
                for j in (0..i).rev() {
                    let t = all_leaves[j];
                    if t.start_line != line {
                        continue;
                    }
                    if !found_copyright {
                        if t.tag == PosTag::Copy && t.value.eq_ignore_ascii_case("copyright") {
                            found_copyright = true;
                            continue;
                        }
                        found_copyright = false;
                        break;
                    }
                    found_copyright = false;
                    break;
                }
                found_copyright
            } else {
                false
            };

        if token.tag == PosTag::Copy || token.tag == PosTag::SpdxContrib {
            if token.tag == PosTag::Copy
                && token.value.eq_ignore_ascii_case("(c)")
                && i > 0
                && all_leaves[i - 1].tag == PosTag::Portions
            {
                i += 1;
                continue;
            }

            if has_line_start_copyright_prefix {
                i += 1;
                continue;
            }
            let mut start = i;

            if token.tag == PosTag::Copy
                && token.value.eq_ignore_ascii_case("copyright")
                && start > 0
                && all_leaves[start - 1].tag == PosTag::Portions
                && all_leaves[start - 1].start_line == token.start_line
            {
                start -= 1;
            }

            if token.tag == PosTag::Copy
                && token.value.eq_ignore_ascii_case("(c)")
                && start > 0
                && all_leaves[start - 1].tag == PosTag::Copy
                && all_leaves[start - 1]
                    .value
                    .eq_ignore_ascii_case("copyright")
                && all_leaves[start - 1].start_line == token.start_line
                && start > 1
                && all_leaves[start - 2].tag == PosTag::Portions
                && all_leaves[start - 2].start_line == token.start_line
            {
                start -= 2;
            }

            if allow_not_copyrighted_prefix && start > 0 {
                let prev = all_leaves[start - 1];
                if prev.start_line == token.start_line && prev.value.eq_ignore_ascii_case("not") {
                    start -= 1;
                }
            }

            let copy_start = start;
            let copy_idx = i;
            i += 1;
            let mut allow_merge_following_copyright_clause = true;
            while i < all_leaves.len()
                && detector::token_utils::is_copyright_span_token(all_leaves[i])
            {
                if all_leaves[i].tag == PosTag::Copy && i > start + 1 {
                    if allow_merge_following_copyright_clause
                        && detector::token_utils::should_merge_following_copyright_clause(
                            &all_leaves,
                            start,
                            i,
                        )
                    {
                        allow_merge_following_copyright_clause = false;
                        i += 1;
                        continue;
                    }
                    if detector::token_utils::should_merge_following_c_sign_after_year(
                        &all_leaves,
                        start,
                        i,
                    ) {
                        i += 1;
                        continue;
                    }
                    break;
                }
                i += 1;
            }

            let mut skip_holder_from_span = false;

            if token.tag == PosTag::Copy
                && token.value.eq_ignore_ascii_case("(c)")
                && copy_start == copy_idx
                && all_leaves[copy_idx..i]
                    .iter()
                    .any(|t| detector::token_utils::YEAR_LIKE_POS_TAGS.contains(&t.tag))
                && !all_leaves[copy_idx..i].iter().any(|t| {
                    matches!(
                        t.tag,
                        PosTag::Nnp
                            | PosTag::Nn
                            | PosTag::Caps
                            | PosTag::Pn
                            | PosTag::MixedCap
                            | PosTag::Comp
                            | PosTag::Uni
                    )
                })
            {
                let line = token.start_line;
                let has_holderish_before = all_leaves[..copy_idx]
                    .iter()
                    .rev()
                    .take_while(|t| t.start_line == line)
                    .any(|t| {
                        matches!(
                            t.tag,
                            PosTag::Nnp
                                | PosTag::Nn
                                | PosTag::Caps
                                | PosTag::Pn
                                | PosTag::MixedCap
                                | PosTag::Comp
                                | PosTag::Uni
                        )
                    });
                if has_holderish_before {
                    while start > 0
                        && all_leaves[start - 1].start_line == line
                        && detector::token_utils::is_copyright_span_token(all_leaves[start - 1])
                    {
                        start -= 1;
                    }
                    skip_holder_from_span = start < copy_start;
                }
            }

            let span = &all_leaves[start..i];
            if span.len() > 1 {
                let allow_single_word_contributors = span
                    .iter()
                    .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr));
                let filtered = detector::token_utils::strip_all_rights_reserved_slice(span);
                if let Some(det) = detector::token_utils::build_copyright_from_tokens(&filtered) {
                    copyrights.push(det);
                }

                if detector::token_utils::is_copyright_of_header(span) {
                    continue;
                }

                if !skip_holder_from_span {
                    let holder_tokens: Vec<&Token> = span
                        .iter()
                        .copied()
                        .filter(|t| !detector::NON_HOLDER_POS_TAGS.contains(&t.tag))
                        .collect();
                    if let Some(det) = detector::token_utils::build_holder_from_tokens(
                        &holder_tokens,
                        allow_single_word_contributors,
                    ) {
                        holders.push(det);
                    } else {
                        let holder_tokens_mini: Vec<&Token> = span
                            .iter()
                            .copied()
                            .filter(|t| !detector::NON_HOLDER_POS_TAGS_MINI.contains(&t.tag))
                            .collect();
                        if let Some(det) = detector::token_utils::build_holder_from_tokens(
                            &holder_tokens_mini,
                            allow_single_word_contributors,
                        ) {
                            holders.push(det);
                        }
                    }
                }
            }
        } else if matches!(
            token.tag,
            PosTag::Auth
                | PosTag::Auths
                | PosTag::AuthDot
                | PosTag::Contributors
                | PosTag::Commit
                | PosTag::SpdxContrib
        ) {
            let start = i;
            let start_line = token.start_line;
            i += 1;
            while i < all_leaves.len() && detector::token_utils::is_author_span_token(all_leaves[i])
            {
                let t = all_leaves[i];
                if t.start_line != start_line {
                    let v = t
                        .value
                        .trim_matches(|c: char| c.is_ascii_punctuation())
                        .to_ascii_lowercase();
                    if matches!(v.as_str(), "date" | "purpose" | "description") {
                        break;
                    }
                    if matches!(
                        t.tag,
                        PosTag::Auth
                            | PosTag::Auths
                            | PosTag::AuthDot
                            | PosTag::Contributors
                            | PosTag::Commit
                            | PosTag::SpdxContrib
                    ) {
                        break;
                    }
                }
                i += 1;
            }

            let span = &all_leaves[start..i];
            if span.len() > 1 {
                let author_tokens: Vec<&Token> = span
                    .iter()
                    .copied()
                    .filter(|t| !detector::NON_AUTHOR_POS_TAGS.contains(&t.tag))
                    .collect();
                if let Some(det) = detector::token_utils::build_author_from_tokens(&author_tokens)
                    && !detector::token_utils::looks_like_bad_generic_author_candidate(&det.author)
                {
                    authors.push(det);
                }
            }
        } else {
            i += 1;
        }
    }

    (copyrights, holders, authors)
}

pub fn extract_copyrights_from_spans(
    tree: &[ParseNode],
    allow_not_copyrighted_prefix: bool,
) -> (Vec<CopyrightDetection>, Vec<HolderDetection>) {
    let mut copyrights: Vec<CopyrightDetection> = Vec::new();
    let mut holders: Vec<HolderDetection> = Vec::new();

    let all_leaves: Vec<&Token> = tree.iter().flat_map(detector::collect_all_leaves).collect();
    if all_leaves.is_empty() {
        return (copyrights, holders);
    }

    let mut i = 0;
    while i < all_leaves.len() {
        let token = all_leaves[i];

        if token.tag == PosTag::Copy || token.tag == PosTag::SpdxContrib {
            if token.tag == PosTag::Copy
                && token.value.eq_ignore_ascii_case("(c)")
                && i > 0
                && all_leaves[i - 1].tag == PosTag::Portions
            {
                i += 1;
                continue;
            }

            let mut start = i;

            if token.tag == PosTag::Copy
                && token.value.eq_ignore_ascii_case("copyright")
                && start > 0
                && all_leaves[start - 1].tag == PosTag::Portions
                && all_leaves[start - 1].start_line == token.start_line
            {
                start -= 1;
            }

            if token.tag == PosTag::Copy
                && token.value.eq_ignore_ascii_case("(c)")
                && start > 0
                && all_leaves[start - 1].tag == PosTag::Copy
                && all_leaves[start - 1]
                    .value
                    .eq_ignore_ascii_case("copyright")
                && all_leaves[start - 1].start_line == token.start_line
            {
                start -= 1;

                if start > 0
                    && all_leaves[start - 1].tag == PosTag::Portions
                    && all_leaves[start - 1].start_line == token.start_line
                {
                    start -= 1;
                }
            }

            if allow_not_copyrighted_prefix && start > 0 {
                let prev = all_leaves[start - 1];
                if prev.start_line == token.start_line && prev.value.eq_ignore_ascii_case("not") {
                    start -= 1;
                }
            }

            let copy_start = start;
            let copy_idx = i;
            i += 1;
            let mut allow_merge_following_copyright_clause = true;
            while i < all_leaves.len()
                && detector::token_utils::is_copyright_span_token(all_leaves[i])
            {
                if all_leaves[i].tag == PosTag::Copy && i > start + 1 {
                    if allow_merge_following_copyright_clause
                        && detector::token_utils::should_merge_following_copyright_clause(
                            &all_leaves,
                            start,
                            i,
                        )
                    {
                        allow_merge_following_copyright_clause = false;
                        i += 1;
                        continue;
                    }
                    if detector::token_utils::should_merge_following_c_sign_after_year(
                        &all_leaves,
                        start,
                        i,
                    ) {
                        i += 1;
                        continue;
                    }
                    break;
                }
                i += 1;
            }

            let mut skip_holder_from_span = false;

            if token.tag == PosTag::Copy
                && token.value.eq_ignore_ascii_case("(c)")
                && copy_start == copy_idx
                && all_leaves[copy_idx..i]
                    .iter()
                    .any(|t| detector::token_utils::YEAR_LIKE_POS_TAGS.contains(&t.tag))
                && !all_leaves[copy_idx..i].iter().any(|t| {
                    matches!(
                        t.tag,
                        PosTag::Nnp
                            | PosTag::Nn
                            | PosTag::Caps
                            | PosTag::Pn
                            | PosTag::MixedCap
                            | PosTag::Comp
                            | PosTag::Uni
                    )
                })
            {
                let line = token.start_line;
                let has_holderish_before = all_leaves[..copy_idx]
                    .iter()
                    .rev()
                    .take_while(|t| t.start_line == line)
                    .any(|t| {
                        matches!(
                            t.tag,
                            PosTag::Nnp
                                | PosTag::Nn
                                | PosTag::Caps
                                | PosTag::Pn
                                | PosTag::MixedCap
                                | PosTag::Comp
                                | PosTag::Uni
                        )
                    });
                if has_holderish_before {
                    while start > 0
                        && all_leaves[start - 1].start_line == line
                        && detector::token_utils::is_copyright_span_token(all_leaves[start - 1])
                    {
                        start -= 1;
                    }
                    skip_holder_from_span = start < copy_start;
                }
            }

            let span = &all_leaves[start..i];
            if span.len() > 1 {
                let allow_single_word_contributors = span
                    .iter()
                    .any(|t| matches!(t.tag, PosTag::Yr | PosTag::YrPlus | PosTag::BareYr));

                let filtered = detector::token_utils::strip_all_rights_reserved_slice(span);
                if let Some(det) = detector::token_utils::build_copyright_from_tokens(&filtered) {
                    copyrights.push(det);
                }

                if detector::token_utils::is_copyright_of_header(span) {
                    continue;
                }

                if !skip_holder_from_span {
                    let holder_tokens: Vec<&Token> = span
                        .iter()
                        .copied()
                        .filter(|t| !detector::NON_HOLDER_POS_TAGS.contains(&t.tag))
                        .collect();
                    if let Some(det) = detector::token_utils::build_holder_from_tokens(
                        &holder_tokens,
                        allow_single_word_contributors,
                    ) {
                        holders.push(det);
                    } else {
                        let holder_tokens_mini: Vec<&Token> = span
                            .iter()
                            .copied()
                            .filter(|t| !detector::NON_HOLDER_POS_TAGS_MINI.contains(&t.tag))
                            .collect();
                        if let Some(det) = detector::token_utils::build_holder_from_tokens(
                            &holder_tokens_mini,
                            allow_single_word_contributors,
                        ) {
                            holders.push(det);
                        }
                    }
                }
            }
        } else {
            i += 1;
        }
    }

    (copyrights, holders)
}
