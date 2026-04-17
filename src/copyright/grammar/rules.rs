//! Grammar rule data for copyright parse tree construction.

use super::types::{GrammarRule, TagMatcher};
use crate::copyright::types::{PosTag, TreeLabel};

// Convenience aliases to keep rule definitions concise.
use PosTag::*;
use TagMatcher::*;
use TreeLabel::*;

/// All grammar rules in application order. Rules are tried sequentially;
/// first match wins at each position. The parser iterates until no more
/// rules fire.
pub(crate) static GRAMMAR_RULES: &[GrammarRule] = &[
    // =========================================================================
    // YEAR RULES (Python lines 2373–2387)
    // =========================================================================
    //
    // #20  YR-RANGE: {<YR>+ <CC>+ <YR>}
    // Expanded: YR CC YR, YR YR CC YR, YR CC CC YR
    GrammarRule {
        label: YrRange,
        pattern: &[Tag(Yr), Tag(Cc), Tag(Yr)],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[Tag(Yr), Tag(Yr), Tag(Cc), Tag(Yr)],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[Tag(Yr), Tag(Cc), Tag(Cc), Tag(Yr)],
    },
    //
    // #30  YR-RANGE: {<YR> <DASH|TO>* <YR|BARE-YR>+}
    // Expanded: YR YR, YR BARE-YR, YR DASH YR, YR TO YR, YR DASH BARE-YR, YR TO BARE-YR
    GrammarRule {
        label: YrRange,
        pattern: &[Tag(Yr), AnyTag(&[Yr, BareYr])],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[Tag(Yr), AnyTag(&[Dash, To]), AnyTag(&[Yr, BareYr])],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[
            Tag(Yr),
            AnyTag(&[Dash, To]),
            AnyTag(&[Yr, BareYr]),
            AnyTag(&[Yr, BareYr]),
        ],
    },
    //
    // #40  YR-RANGE: {<CD|CDS|BARE-YR>? <YR> <BARE-YR>?}
    // Expanded: YR, CD YR, CDS YR, BARE-YR YR, YR BARE-YR, CD YR BARE-YR, CDS YR BARE-YR, BARE-YR YR BARE-YR
    GrammarRule {
        label: YrRange,
        pattern: &[Tag(Yr)],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[AnyTag(&[Cd, Cds, BareYr]), Tag(Yr)],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[Tag(Yr), Tag(BareYr)],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[AnyTag(&[Cd, Cds, BareYr]), Tag(Yr), Tag(BareYr)],
    },
    //
    // #50  YR-RANGE: {<YR>+ <BARE-YR>?}
    // Expanded: YR (already covered by #40), YR YR, YR BARE-YR (covered), YR YR BARE-YR
    GrammarRule {
        label: YrRange,
        pattern: &[Tag(Yr), Tag(Yr)],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[Tag(Yr), Tag(Yr), Tag(BareYr)],
    },
    //
    // #60  YR-AND: {<CC>? <YR>+ <CC>+ <YR>}
    // Expanded: YR CC YR (covered by #20 as YrRange, but this is YrAnd),
    //           CC YR CC YR, YR YR CC YR, CC YR YR CC YR
    GrammarRule {
        label: YrAnd,
        pattern: &[Tag(Yr), Tag(Cc), Tag(Yr)],
    },
    GrammarRule {
        label: YrAnd,
        pattern: &[Tag(Cc), Tag(Yr), Tag(Cc), Tag(Yr)],
    },
    GrammarRule {
        label: YrAnd,
        pattern: &[Tag(Yr), Tag(Yr), Tag(Cc), Tag(Yr)],
    },
    GrammarRule {
        label: YrAnd,
        pattern: &[Tag(Cc), Tag(Yr), Tag(Yr), Tag(Cc), Tag(Yr)],
    },
    //
    // #70  YR-RANGE: {<YR-AND>+}
    GrammarRule {
        label: YrRange,
        pattern: &[Label(YrAnd)],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[Label(YrAnd), Label(YrAnd)],
    },
    //
    // #71  YR-RANGE: {<YR-RANGE>+ <DASH|TO> <YR-RANGE>+}
    GrammarRule {
        label: YrRange,
        pattern: &[Label(YrRange), AnyTag(&[Dash, To]), Label(YrRange)],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[
            Label(YrRange),
            Label(YrRange),
            AnyTag(&[Dash, To]),
            Label(YrRange),
        ],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[
            Label(YrRange),
            AnyTag(&[Dash, To]),
            Label(YrRange),
            Label(YrRange),
        ],
    },
    //
    // #72  YR-RANGE: {<YR-RANGE>+ <DASH>?}
    // Expanded: YR-RANGE (already covered), YR-RANGE DASH, YR-RANGE YR-RANGE, YR-RANGE YR-RANGE DASH
    GrammarRule {
        label: YrRange,
        pattern: &[Label(YrRange), Tag(Dash)],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[Label(YrRange), Label(YrRange)],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[Label(YrRange), Label(YrRange), Tag(Dash)],
    },
    //
    // #72.2  YR-RANGE: {<YR-RANGE> <CD|CDS>+}
    GrammarRule {
        label: YrRange,
        pattern: &[Label(YrRange), AnyTag(&[Cd, Cds])],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[Label(YrRange), AnyTag(&[Cd, Cds]), AnyTag(&[Cd, Cds])],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[
            Label(YrRange),
            AnyTag(&[Cd, Cds]),
            AnyTag(&[Cd, Cds]),
            AnyTag(&[Cd, Cds]),
        ],
    },
    // #72.2.1  YR-RANGE: {<YR-RANGE> <CC> <CD|CDS>}
    // Handles comma-separated short years after a full year range, e.g.
    // "1999, 2000, 01, 03, 06" where the lexer splits "01," into "01" + ","
    GrammarRule {
        label: YrRange,
        pattern: &[Label(YrRange), Tag(Cc), AnyTag(&[Cd, Cds])],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[
            Label(YrRange),
            Tag(Cc),
            AnyTag(&[Cd, Cds]),
            Tag(Cc),
            AnyTag(&[Cd, Cds]),
        ],
    },
    GrammarRule {
        label: YrRange,
        pattern: &[
            Label(YrRange),
            Tag(Cc),
            AnyTag(&[Cd, Cds]),
            Tag(Cc),
            AnyTag(&[Cd, Cds]),
            Tag(Cc),
            AnyTag(&[Cd, Cds]),
        ],
    },
    //
    // #bareyear  CD: {<BARE-YR>}
    // Note: This re-tags BARE-YR as CD. We model it as a rule that produces
    // a special label. Since CD is a PosTag not a TreeLabel, we handle this
    // in the parser by re-tagging. For now we skip this rule as it changes
    // a tag rather than creating a tree node. The parser will handle BARE-YR
    // to CD promotion separately.
    //
    // #72.3  YR-RANGE: {<CDS> <NNP> <YR-RANGE>}
    GrammarRule {
        label: YrRange,
        pattern: &[Tag(Cds), Tag(Nnp), Label(YrRange)],
    },
    // =========================================================================
    // ALL RIGHTS RESERVED (Python line 2395)
    // =========================================================================
    //
    // ALLRIGHTRESERVED: {<NNP|NN|CAPS> <RIGHT> <NNP|NN|CAPS>? <RESERVED>}
    // Expanded: with and without the optional middle element
    GrammarRule {
        label: AllRightReserved,
        pattern: &[AnyTag(&[Nnp, Nn, Caps]), Tag(Right), Tag(Reserved)],
    },
    GrammarRule {
        label: AllRightReserved,
        pattern: &[
            AnyTag(&[Nnp, Nn, Caps]),
            Tag(Right),
            AnyTag(&[Nnp, Nn, Caps]),
            Tag(Reserved),
        ],
    },
    // =========================================================================
    // COMPOSITE EMAIL RULES (Python lines 2401–2415)
    // =========================================================================
    //
    // EMAIL: {<EMAIL_START> <CC> <NN>* <EMAIL_END>}
    // Expanded: without NN, with 1 NN, with 2 NN
    GrammarRule {
        label: TreeLabel::Name,
        pattern: &[Tag(EmailStart), Tag(Cc), Tag(EmailEnd)],
    },
    GrammarRule {
        label: TreeLabel::Name,
        pattern: &[Tag(EmailStart), Tag(Cc), Tag(Nn), Tag(EmailEnd)],
    },
    GrammarRule {
        label: TreeLabel::Name,
        pattern: &[Tag(EmailStart), Tag(Cc), Tag(Nn), Tag(Nn), Tag(EmailEnd)],
    },
    // Note: The Python grammar labels these as EMAIL but we don't have an Email
    // TreeLabel — Email is a PosTag. We'll use a special approach: the parser
    // will re-tag composite emails. For now, we model EMAIL grammar rules as
    // producing Name nodes (the parser will handle the EMAIL→Name promotion).
    // Actually, let's reconsider: these rules BUILD composite emails from parts.
    // The result should be treated as an Email token. Since our TreeLabel doesn't
    // have Email, and the Python grammar uses EMAIL as both a POS tag and a
    // chunk label, we need to handle this carefully.
    //
    // DECISION: We'll skip the EMAIL composite rules from the grammar and handle
    // email composition in the lexer/parser directly, since Email is a POS tag.
    // The grammar rules below that REFERENCE <EMAIL> will use Tag(Email).
    //
    // Let me reconsider and include them properly. The Python grammar treats
    // EMAIL as a chunk label that can be referenced by later rules. In our
    // system, we need a way to represent this. Since we already have Tag(Email)
    // for leaf tokens, and the grammar builds composite emails, we should
    // handle this in the parser by re-tagging the result as a leaf Email token.
    // For the grammar file, we'll omit these EMAIL-building rules and note
    // that the parser handles email composition separately.

    // =========================================================================
    // CC RULE (Python line 2422)
    // =========================================================================
    //
    // #73  CC: {<CC><CC>}
    // Two CC tokens merge into one. Like EMAIL, CC is a PosTag not a TreeLabel.
    // The parser will handle CC merging as a special re-tag operation.

    // =========================================================================
    // DASHCAPS (Python line 2457)
    // =========================================================================
    //
    // #899999  DASHCAPS: {<DASH> <CAPS>}
    GrammarRule {
        label: DashCaps,
        pattern: &[Tag(Dash), Tag(Caps)],
    },
    // =========================================================================
    // NAME RULES (Python lines 2424–2456, 2540–2598, 2602–2606, 2630–2698,
    //             2729, 2752–2754, 2761, 2764, 2767, 2819, 2825, 2842–2851,
    //             2860, 2866)
    // =========================================================================
    //
    // #75  NAME: {<NAME><NNP>}
    GrammarRule {
        label: Name,
        pattern: &[Label(Name), Tag(Nnp)],
    },
    //
    // #80  NAME: {<NN|NNP> <CC> <URL|URL2>}
    GrammarRule {
        label: Name,
        pattern: &[AnyTag(&[Nn, Nnp]), Tag(Cc), AnyTag(&[Url, Url2])],
    },
    //
    // #88  NAME: {<NNP>+ <VAN|OF> <NNP>+}
    // Expanded: NNP VAN/OF NNP, NNP NNP VAN/OF NNP, NNP VAN/OF NNP NNP
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), AnyTag(&[Van, Of]), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Nnp), AnyTag(&[Van, Of]), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), AnyTag(&[Van, Of]), Tag(Nnp), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Nnp), AnyTag(&[Van, Of]), Tag(Nnp), Tag(Nnp)],
    },
    //
    // #90  NAME: {<NNP> <VAN|OF> <NN*> <NNP>}
    // Note: <NN*> likely means <NN>* (zero or more NN). Expand: without NN, with 1 NN
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), AnyTag(&[Van, Of]), Tag(Nnp)],
    }, // duplicate of #88 base, but order matters
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), AnyTag(&[Van, Of]), Tag(Nn), Tag(Nnp)],
    },
    //
    // #100  NAME: {<NNP> <PN> <VAN> <NNP>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Pn), Tag(Van), Tag(Nnp)],
    },
    //
    // #110  NAME: {<BY> <NN>+ <EMAIL>}
    // Expanded: BY NN EMAIL, BY NN NN EMAIL
    GrammarRule {
        label: Name,
        pattern: &[Tag(By), Tag(Nn), Tag(Email)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(By), Tag(Nn), Tag(Nn), Tag(Email)],
    },
    //
    // #120  NAME: {<NNP> <PN> <CAPS>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Pn), Tag(Caps)],
    },
    //
    // #121  NAME: {<NNP> <CAPS> <NNP>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Caps), Tag(Nnp)],
    },
    //
    // #85  NAME: {<BY> <CAPS> <PN> <CAPS>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(By), Tag(Caps), Tag(Pn), Tag(Caps)],
    },
    //
    // #340  NAME: {<NNP> <NNP> <MIXEDCAP>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Nnp), Tag(MixedCap)],
    },
    //
    // #345  NAME: {<NNP> <NNP> <CAPS>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Nnp), Tag(Caps)],
    },
    //
    // #345.1  NAME: {<NNP> <NNP> <CC> <NNP> <NN> <NNP>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Nnp), Tag(Cc), Tag(Nnp), Tag(Nn), Tag(Nnp)],
    },
    //
    // #346  NAME: {<NNP> <NNP> <CC> <NNP>+}
    // Expanded: NNP NNP CC NNP, NNP NNP CC NNP NNP
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Nnp), Tag(Cc), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Nnp), Tag(Cc), Tag(Nnp), Tag(Nnp)],
    },
    //
    // #350.3  EMAIL: {<NAME|NNP|NN> <AT> <NN|NNP> <DOT> <NN|NNP>}
    // This builds a composite email from name + AT + domain. Since EMAIL is a
    // PosTag, the parser handles re-tagging. We include it as a Name rule since
    // the result is used in name contexts.
    // (Handled by parser as email composition)
    //
    // #351  NAME: {<NNP|PN>+ <NNP>+}
    // Expanded: NNP NNP (covered), PN NNP, NNP PN NNP, PN PN NNP, NNP NNP NNP
    GrammarRule {
        label: Name,
        pattern: &[Tag(Pn), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[AnyTag(&[Nnp, Pn]), AnyTag(&[Nnp, Pn]), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[AnyTag(&[Nnp, Pn]), AnyTag(&[Nnp, Pn]), Tag(Nnp), Tag(Nnp)],
    },
    //
    // #881111  NAME: {<NN> <NNP>{3}}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nn), Tag(Nnp), Tag(Nnp), Tag(Nnp)],
    },
    //
    // #351.1  NAME: {<NN>? <NN>? <EMAIL> <NAME>}
    // Expanded: EMAIL NAME, NN EMAIL NAME, NN NN EMAIL NAME
    GrammarRule {
        label: Name,
        pattern: &[Tag(Email), Label(Name)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nn), Tag(Email), Label(Name)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nn), Tag(Nn), Tag(Email), Label(Name)],
    },
    //
    // #352  NAME: {<NNP> <CAPS>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Caps)],
    },
    //
    // #353  NAME: {<NNP> <PN>+}
    // Expanded: NNP PN, NNP PN PN
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Pn)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Pn), Tag(Pn)],
    },
    //
    // #390  NAME: {<NNP> <NN|NNP> <EMAIL>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), AnyTag(&[Nn, Nnp]), Tag(Email)],
    },
    //
    // #400  NAME: {<NNP> <PN|VAN>? <PN|VAN>? <NNP>}
    // Expanded: NNP NNP (covered), NNP PN/VAN NNP, NNP PN/VAN PN/VAN NNP
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), AnyTag(&[Pn, Van]), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), AnyTag(&[Pn, Van]), AnyTag(&[Pn, Van]), Tag(Nnp)],
    },
    //
    // #410  NAME: {<NNP> <NN> <NNP>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Nn), Tag(Nnp)],
    },
    //
    // #420  NAME: {<NNP> <COMMIT>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Commit)],
    },
    //
    // #440  NAME: {<NN>? <NNP> <MAINT> <COMP>}
    // Expanded: NNP MAINT COMP, NN NNP MAINT COMP
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Maint), Tag(Comp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nn), Tag(Nnp), Tag(Maint), Tag(Comp)],
    },
    //
    // #460  NAME: {<NNP> <NN>? <MAINT>}
    // Expanded: NNP MAINT, NNP NN MAINT
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Maint)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Nn), Tag(Maint)],
    },
    //
    // #480  NAME: {<NN>? <NNP> <CC> <NAME>}
    // Expanded: NNP CC NAME, NN NNP CC NAME
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Cc), Label(Name)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nn), Tag(Nnp), Tag(Cc), Label(Name)],
    },
    //
    // #490  NAME: {<NN>? <NNP> <OF> <NN>? <NNP> <NNP>?}
    // Expanded (most common forms):
    //   NNP OF NNP, NNP OF NNP NNP, NN NNP OF NNP, NN NNP OF NNP NNP,
    //   NNP OF NN NNP, NNP OF NN NNP NNP, NN NNP OF NN NNP, NN NNP OF NN NNP NNP
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Of), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Of), Tag(Nnp), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nn), Tag(Nnp), Tag(Of), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nn), Tag(Nnp), Tag(Of), Tag(Nnp), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Of), Tag(Nn), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Of), Tag(Nn), Tag(Nnp), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nn), Tag(Nnp), Tag(Of), Tag(Nn), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nn), Tag(Nnp), Tag(Of), Tag(Nn), Tag(Nnp), Tag(Nnp)],
    },
    //
    // #350again  NAME: {<NNP|PN>+ <CC>+ <NNP>+}
    // Expanded: NNP/PN CC NNP, NNP/PN NNP/PN CC NNP, NNP/PN CC NNP NNP
    GrammarRule {
        label: Name,
        pattern: &[AnyTag(&[Nnp, Pn]), Tag(Cc), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[AnyTag(&[Nnp, Pn]), AnyTag(&[Nnp, Pn]), Tag(Cc), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[AnyTag(&[Nnp, Pn]), Tag(Cc), Tag(Nnp), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[
            AnyTag(&[Nnp, Pn]),
            AnyTag(&[Nnp, Pn]),
            Tag(Cc),
            Tag(Nnp),
            Tag(Nnp),
        ],
    },
    //
    // #500  NAME: {<NAME> <CC> <NAME>}
    GrammarRule {
        label: Name,
        pattern: &[Label(Name), Tag(Cc), Label(Name)],
    },
    //
    // #480 (second)  NAME: {<CC> <NNP> <MIXEDCAP>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Cc), Tag(Nnp), Tag(MixedCap)],
    },
    //
    // #483  NAME: {<NAME> <UNI>}
    GrammarRule {
        label: Name,
        pattern: &[Label(Name), Tag(Uni)],
    },
    //
    // #550  NAME: {<NAME|NAME-EMAIL>+ <OF> <NNP> <OF> <NN>? <COMPANY>}
    // Expanded: NAME OF NNP OF COMPANY, NAME OF NNP OF NN COMPANY,
    //           NAME-EMAIL OF NNP OF COMPANY, NAME-EMAIL OF NNP OF NN COMPANY
    GrammarRule {
        label: Name,
        pattern: &[
            AnyLabel(&[Name, NameEmail]),
            Tag(Of),
            Tag(Nnp),
            Tag(Of),
            Label(Company),
        ],
    },
    GrammarRule {
        label: Name,
        pattern: &[
            AnyLabel(&[Name, NameEmail]),
            Tag(Of),
            Tag(Nnp),
            Tag(Of),
            Tag(Nn),
            Label(Company),
        ],
    },
    //
    // #560  NAME: {<NAME|NAME-EMAIL>+ <CC|OF>? <NAME|NAME-EMAIL|COMPANY>}
    // Expanded: NAME NAME/NAME-EMAIL/COMPANY, NAME CC/OF NAME/NAME-EMAIL/COMPANY,
    //           NAME-EMAIL NAME/..., NAME-EMAIL CC/OF NAME/...
    GrammarRule {
        label: Name,
        pattern: &[
            AnyLabel(&[Name, NameEmail]),
            AnyLabel(&[Name, NameEmail, Company]),
        ],
    },
    GrammarRule {
        label: Name,
        pattern: &[
            AnyLabel(&[Name, NameEmail]),
            AnyTag(&[Cc, Of]),
            AnyLabel(&[Name, NameEmail, Company]),
        ],
    },
    //
    // #561  NAME: {<NNP><NNP>}
    // Already covered by #400 expansion (NNP NNP)
    //
    // #561.3  NAME: {<NNP> <OF> <VAN> <NNP>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Of), Tag(Van), Tag(Nnp)],
    },
    //
    // #563  NAME: {<NAME> <CC> <NNP>}
    GrammarRule {
        label: Name,
        pattern: &[Label(Name), Tag(Cc), Tag(Nnp)],
    },
    //
    // #566  NAME: {<PORTIONS> <OF> <NN> <NAME>+}
    // Expanded: PORTIONS OF NN NAME, PORTIONS OF NN NAME NAME
    GrammarRule {
        label: Name,
        pattern: &[Tag(Portions), Tag(Of), Tag(Nn), Label(Name)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Portions), Tag(Of), Tag(Nn), Label(Name), Label(Name)],
    },
    //
    // #580  NAME: {<NNP> <OF> <NNP>}
    // Already covered by #490 expansion
    //
    // #590  NAME: {<NAME> <NNP>}
    // Already covered by #75
    //
    // #600  NAME: {<NN|NNP|CAPS>+ <CC> <OTH>}
    // Expanded: NN/NNP/CAPS CC OTH, NN/NNP/CAPS NN/NNP/CAPS CC OTH
    GrammarRule {
        label: Name,
        pattern: &[AnyTag(&[Nn, Nnp, Caps]), Tag(Cc), Tag(Oth)],
    },
    GrammarRule {
        label: Name,
        pattern: &[
            AnyTag(&[Nn, Nnp, Caps]),
            AnyTag(&[Nn, Nnp, Caps]),
            Tag(Cc),
            Tag(Oth),
        ],
    },
    //
    // #610  NAME: {<NNP> <CAPS>}
    // Already covered by #352
    //
    // #620  NAME: {<CAPS> <DASH>? <NNP|NAME>}
    // Expanded: CAPS NNP, CAPS NAME, CAPS DASH NNP, CAPS DASH NAME
    GrammarRule {
        label: Name,
        pattern: &[Tag(Caps), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Caps), Label(Name)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Caps), Tag(Dash), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Caps), Tag(Dash), Label(Name)],
    },
    //
    // #630  NAME: {<NNP> <CD|CDS> <NNP>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), AnyTag(&[Cd, Cds]), Tag(Nnp)],
    },
    //
    // #640  NAME: {<COMP> <NAME>+}
    // Expanded: COMP NAME, COMP NAME NAME
    GrammarRule {
        label: Name,
        pattern: &[Tag(Comp), Label(Name)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Comp), Label(Name), Label(Name)],
    },
    //
    // #644  NAME: {<AUTHS>? <CC> <NN>? <CONTRIBUTORS>}
    // Expanded: CC CONTRIBUTORS, CC NN CONTRIBUTORS, AUTHS CC CONTRIBUTORS, AUTHS CC NN CONTRIBUTORS
    GrammarRule {
        label: Name,
        pattern: &[Tag(Cc), Tag(Contributors)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Cc), Tag(Nn), Tag(Contributors)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Auths), Tag(Cc), Tag(Contributors)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Auths), Tag(Cc), Tag(Nn), Tag(Contributors)],
    },
    //
    // #660  NAME: {<NNP|CAPS>+ <AUTHS|AUTHDOT|CONTRIBUTORS>}
    // Expanded: NNP/CAPS AUTHS/AUTHDOT/CONTRIBUTORS, NNP/CAPS NNP/CAPS AUTHS/...
    GrammarRule {
        label: Name,
        pattern: &[
            AnyTag(&[Nnp, Caps]),
            AnyTag(&[Auths, AuthDot, Contributors]),
        ],
    },
    GrammarRule {
        label: Name,
        pattern: &[
            AnyTag(&[Nnp, Caps]),
            AnyTag(&[Nnp, Caps]),
            AnyTag(&[Auths, AuthDot, Contributors]),
        ],
    },
    GrammarRule {
        label: Name,
        pattern: &[
            AnyTag(&[Nnp, Caps]),
            AnyTag(&[Nnp, Caps]),
            AnyTag(&[Nnp, Caps]),
            AnyTag(&[Auths, AuthDot, Contributors]),
        ],
    },
    //
    // #680  NAME: {<VAN|OF> <NAME>}
    GrammarRule {
        label: Name,
        pattern: &[AnyTag(&[Van, Of]), Label(Name)],
    },
    //
    // #690  NAME: {<NAME-YEAR> <COMP|COMPANY>}
    GrammarRule {
        label: Name,
        pattern: &[Label(NameYear), AnyTagOrLabel(&[Comp], &[Company])],
    },
    // #690.1  Rust-specific: NAME-YEAR CC COMP/COMPANY → NAME
    // Handles "2008 Foo Name, Inc." where the comma is split out in Rust
    // tokenization (Python keeps "Name," as a single NNP token).
    GrammarRule {
        label: Name,
        pattern: &[Label(NameYear), Tag(Cc), AnyTagOrLabel(&[Comp], &[Company])],
    },
    //
    // #710  NAME: {<NNP> <NAME>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Label(Name)],
    },
    //
    // #720  NAME: {<CC>? <IN> <NAME|NNP>}
    // Expanded: IN NAME, IN NNP, CC IN NAME, CC IN NNP
    GrammarRule {
        label: Name,
        pattern: &[Tag(In), Label(Name)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(In), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Cc), Tag(In), Label(Name)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Cc), Tag(In), Tag(Nnp)],
    },
    //
    // #730  NAME: {<NAME><UNI>}
    // Already covered by #483
    //
    // #740  NAME: {<NAME> <IN> <NNP> <CC|IN>+ <NNP>}
    // Expanded: NAME IN NNP CC/IN NNP, NAME IN NNP CC/IN CC/IN NNP
    GrammarRule {
        label: Name,
        pattern: &[Label(Name), Tag(In), Tag(Nnp), AnyTag(&[Cc, In]), Tag(Nnp)],
    },
    GrammarRule {
        label: Name,
        pattern: &[
            Label(Name),
            Tag(In),
            Tag(Nnp),
            AnyTag(&[Cc, In]),
            AnyTag(&[Cc, In]),
            Tag(Nnp),
        ],
    },
    //
    // #741  NAME: {<BY> <NNP> <URL>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(By), Tag(Nnp), Tag(Url)],
    },
    //
    // #742  NAME: {<NNP> <URL> <EMAIL>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Url), Tag(Email)],
    },
    //
    // #870  NAME: {<NN> <NNP> <OF> <NN> <COMPANY>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nn), Tag(Nnp), Tag(Of), Tag(Nn), Label(Company)],
    },
    //
    // #980  NAME: {<VAN>? <NNP> <ANDCO>+}
    // Expanded: NNP ANDCO, NNP ANDCO ANDCO, VAN NNP ANDCO, VAN NNP ANDCO ANDCO
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Label(AndCo)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Label(AndCo), Label(AndCo)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Van), Tag(Nnp), Label(AndCo)],
    },
    GrammarRule {
        label: Name,
        pattern: &[Tag(Van), Tag(Nnp), Label(AndCo), Label(AndCo)],
    },
    //
    // #1000  NAME: {<BY> <NN> <AUTH|CONTRIBUTORS|AUTHS>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(By), Tag(Nn), AnyTag(&[Auth, Contributors, Auths])],
    },
    //
    // #1060  NAME: {<COMPANY> <OF> <NN|NNP>}
    GrammarRule {
        label: Name,
        pattern: &[Label(Company), Tag(Of), AnyTag(&[Nn, Nnp])],
    },
    //
    // #1090  NAME: {<NAME> <COMPANY>}
    GrammarRule {
        label: Name,
        pattern: &[Label(Name), Label(Company)],
    },
    //
    // #1120  NAME: {<NAME|NNP> <CC> <NNP|NAME>}
    GrammarRule {
        label: Name,
        pattern: &[
            AnyTagOrLabel(&[Nnp], &[Name]),
            Tag(Cc),
            AnyTagOrLabel(&[Nnp], &[Name]),
        ],
    },
    //
    // #1410  NAME: {<NNP> <ANDCO>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Label(AndCo)],
    },
    //
    // #1412  NAME: {<NAME> <CC> <NN> <CONTRIBUTORS>}
    GrammarRule {
        label: Name,
        pattern: &[Label(Name), Tag(Cc), Tag(Nn), Tag(Contributors)],
    },
    //
    // #1960  NAME: {<NN> <NN> <AUTH|CONTRIBUTORS|AUTHS> <NN> <AUTH|CONTRIBUTORS|AUTHS|AUTHDOT>}
    GrammarRule {
        label: Name,
        pattern: &[
            Tag(Nn),
            Tag(Nn),
            AnyTag(&[Auth, Contributors, Auths]),
            Tag(Nn),
            AnyTag(&[Auth, Contributors, Auths, AuthDot]),
        ],
    },
    //
    // #196023  NAME: {<NN> <NAME> <CONTRIBUTORS|AUTHS>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nn), Label(Name), AnyTag(&[Contributors, Auths])],
    },
    //
    // #19601.1  NAME: {<NNP> <PN> <EMAIL>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Nnp), Tag(Pn), Tag(Email)],
    },
    //
    // #19601  NAME: {<NAME> <DASH> <NAME> <CAPS>}
    GrammarRule {
        label: Name,
        pattern: &[Label(Name), Tag(Dash), Label(Name), Tag(Caps)],
    },
    //
    // #19653  NAME: {<PARENS> <NAME> <PARENS>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Parens), Label(Name), Tag(Parens)],
    },
    //
    // #19673  NAME: {<UNI> <OF> <CAPS>}
    GrammarRule {
        label: Name,
        pattern: &[Tag(Uni), Tag(Of), Tag(Caps)],
    },
    // =========================================================================
    // NAME-EMAIL RULES (Python line 2623)
    // =========================================================================
    //
    // #530  NAME-EMAIL: {<NAME> <EMAIL>}
    GrammarRule {
        label: NameEmail,
        pattern: &[Label(Name), Tag(Email)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Junk), Tag(Cc), Tag(Comp)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Nn), Tag(Cc), Tag(Comp)],
    },
    // =========================================================================
    // NAME-YEAR RULES (Python lines 2554, 2625–2669)
    // =========================================================================
    //
    // #350  NAME-YEAR: {<YR-RANGE> <NNP> <NNP>+}
    // Expanded: YR-RANGE NNP NNP, YR-RANGE NNP NNP NNP
    GrammarRule {
        label: NameYear,
        pattern: &[Label(YrRange), Tag(Nnp), Tag(Nnp)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(YrRange), Tag(Nnp), Tag(Nnp), Tag(Nnp)],
    },
    //
    // #535  NAME-YEAR: {<PARENS>? <YR-RANGE> <NAME-EMAIL|COMPANY>+ <NNP>? <PARENS>?}
    // Expanded (most common forms):
    //   YR-RANGE NAME-EMAIL/COMPANY
    //   YR-RANGE NAME-EMAIL/COMPANY NNP
    //   YR-RANGE NAME-EMAIL/COMPANY PARENS
    //   PARENS YR-RANGE NAME-EMAIL/COMPANY
    //   PARENS YR-RANGE NAME-EMAIL/COMPANY NNP
    //   PARENS YR-RANGE NAME-EMAIL/COMPANY NNP PARENS
    //   PARENS YR-RANGE NAME-EMAIL/COMPANY PARENS
    GrammarRule {
        label: NameYear,
        pattern: &[Label(YrRange), AnyLabel(&[NameEmail, Company])],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(YrRange), AnyLabel(&[NameEmail, Company]), Tag(Nnp)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(YrRange), AnyLabel(&[NameEmail, Company]), Tag(Parens)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Tag(Parens), Label(YrRange), AnyLabel(&[NameEmail, Company])],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[
            Tag(Parens),
            Label(YrRange),
            AnyLabel(&[NameEmail, Company]),
            Tag(Nnp),
        ],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[
            Tag(Parens),
            Label(YrRange),
            AnyLabel(&[NameEmail, Company]),
            Tag(Nnp),
            Tag(Parens),
        ],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[
            Tag(Parens),
            Label(YrRange),
            AnyLabel(&[NameEmail, Company]),
            Tag(Parens),
        ],
    },
    //
    // #540  NAME-YEAR: {<YR-RANGE> <NAME-EMAIL|COMPANY>+ <CC> <YR-RANGE>}
    GrammarRule {
        label: NameYear,
        pattern: &[
            Label(YrRange),
            AnyLabel(&[NameEmail, Company]),
            Tag(Cc),
            Label(YrRange),
        ],
    },
    //
    // #561.1  NAME-YEAR: {<NAME>+ <YR-RANGE>}
    // Expanded: NAME YR-RANGE, NAME NAME YR-RANGE
    GrammarRule {
        label: NameYear,
        pattern: &[Label(Name), Label(YrRange)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(Name), Label(Name), Label(YrRange)],
    },
    //
    // #562  NAME-YEAR: {<YR-RANGE> <NNP>+ <CAPS>? <LINUX>?}
    // Expanded: YR-RANGE NNP, YR-RANGE NNP NNP (covered by #350),
    //           YR-RANGE NNP CAPS, YR-RANGE NNP LINUX,
    //           YR-RANGE NNP CAPS LINUX, YR-RANGE NNP NNP CAPS
    GrammarRule {
        label: NameYear,
        pattern: &[Label(YrRange), Tag(Nnp)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(YrRange), Tag(Nnp), Tag(Caps)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(YrRange), Tag(Nnp), Tag(Linux)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(YrRange), Tag(Nnp), Tag(Caps), Tag(Linux)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(YrRange), Tag(Nnp), Tag(Nnp), Tag(Caps)],
    },
    //
    // #570  NAME-YEAR: {<YR-RANGE> <NAME>+ <CONTRIBUTORS>?}
    // Expanded: YR-RANGE NAME, YR-RANGE NAME CONTRIBUTORS,
    //           YR-RANGE NAME NAME, YR-RANGE NAME NAME CONTRIBUTORS
    GrammarRule {
        label: NameYear,
        pattern: &[Label(YrRange), Label(Name)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(YrRange), Label(Name), Tag(Contributors)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(YrRange), Label(Name), Label(Name)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(YrRange), Label(Name), Label(Name), Tag(Contributors)],
    },
    //
    // #5700.1  NAME-YEAR: {<NAME-YEAR> <CDS> <NNP>}
    GrammarRule {
        label: NameYear,
        pattern: &[Label(NameYear), Tag(Cds), Tag(Nnp)],
    },
    //
    // #5701  NAME-YEAR: {<NAME-YEAR> <VAN>? <EMAIL>? <URL>?}
    // Expanded (non-trivial combinations):
    //   NAME-YEAR VAN, NAME-YEAR EMAIL, NAME-YEAR URL,
    //   NAME-YEAR VAN EMAIL, NAME-YEAR VAN URL,
    //   NAME-YEAR EMAIL URL, NAME-YEAR VAN EMAIL URL
    GrammarRule {
        label: NameYear,
        pattern: &[Label(NameYear), Tag(Van)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(NameYear), Tag(Email)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(NameYear), Tag(Url)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(NameYear), Tag(Van), Tag(Email)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(NameYear), Tag(Van), Tag(Url)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(NameYear), Tag(Email), Tag(Url)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(NameYear), Tag(Van), Tag(Email), Tag(Url)],
    },
    //
    // #5701.1  NAME-YEAR: {<NAME-YEAR> <NN> <DASH> <NAME>}
    GrammarRule {
        label: NameYear,
        pattern: &[Label(NameYear), Tag(Nn), Tag(Dash), Label(Name)],
    },
    //
    // #5702  NAME-YEAR: {<NAME-YEAR>+}
    // Expanded: NAME-YEAR NAME-YEAR, NAME-YEAR NAME-YEAR NAME-YEAR
    GrammarRule {
        label: NameYear,
        pattern: &[Label(NameYear), Label(NameYear)],
    },
    GrammarRule {
        label: NameYear,
        pattern: &[Label(NameYear), Label(NameYear), Label(NameYear)],
    },
    // =========================================================================
    // URL RULE (Python line 2658)
    // =========================================================================
    //
    // #5700  URL: {<PARENS> <URL> <PARENS>}
    // URL is a PosTag, not a TreeLabel. The parser handles re-tagging.
    // We skip this rule here; the parser will handle parenthesized URLs.

    // =========================================================================
    // ANDCO RULES (Python lines 2651, 2715, 2738–2744, 2807–2809)
    // =========================================================================
    //
    // #565  ANDCO: {<CC> <NN> <COMPANY>}
    GrammarRule {
        label: AndCo,
        pattern: &[Tag(Cc), Tag(Nn), Label(Company)],
    },
    //
    // #825  ANDCO: {<CC> <NNP>? <NN> <URL>}
    // Expanded: CC NN URL, CC NNP NN URL
    GrammarRule {
        label: AndCo,
        pattern: &[Tag(Cc), Tag(Nn), Tag(Url)],
    },
    GrammarRule {
        label: AndCo,
        pattern: &[Tag(Cc), Tag(Nnp), Tag(Nn), Tag(Url)],
    },
    //
    // #930  ANDCO: {<CC> <NNP> <NNP>+}
    // Expanded: CC NNP NNP, CC NNP NNP NNP
    GrammarRule {
        label: AndCo,
        pattern: &[Tag(Cc), Tag(Nnp), Tag(Nnp)],
    },
    GrammarRule {
        label: AndCo,
        pattern: &[Tag(Cc), Tag(Nnp), Tag(Nnp), Tag(Nnp)],
    },
    //
    // #940  ANDCO: {<CC> <OTH>}
    GrammarRule {
        label: AndCo,
        pattern: &[Tag(Cc), Tag(Oth)],
    },
    //
    // #950  ANDCO: {<CC> <NN> <NAME>+}
    // Expanded: CC NN NAME, CC NN NAME NAME
    GrammarRule {
        label: AndCo,
        pattern: &[Tag(Cc), Tag(Nn), Label(Name)],
    },
    GrammarRule {
        label: AndCo,
        pattern: &[Tag(Cc), Tag(Nn), Label(Name), Label(Name)],
    },
    //
    // #960  ANDCO: {<CC> <CAPS|COMPANY|NAME|NAME-EMAIL|NAME-YEAR>+}
    // Expanded: CC X, CC X X (where X is any of the listed)
    GrammarRule {
        label: AndCo,
        pattern: &[
            Tag(Cc),
            AnyTagOrLabel(&[Caps], &[Company, Name, NameEmail, NameYear]),
        ],
    },
    GrammarRule {
        label: AndCo,
        pattern: &[
            Tag(Cc),
            AnyTagOrLabel(&[Caps], &[Company, Name, NameEmail, NameYear]),
            AnyTagOrLabel(&[Caps], &[Company, Name, NameEmail, NameYear]),
        ],
    },
    //
    // #1430  ANDCO: {<CC>+ <NN> <NNP>+ <UNI|COMP>?}
    // Expanded: CC NN NNP, CC NN NNP COMP/UNI, CC NN NNP NNP, CC NN NNP NNP COMP/UNI
    GrammarRule {
        label: AndCo,
        pattern: &[Tag(Cc), Tag(Nn), Tag(Nnp)],
    },
    GrammarRule {
        label: AndCo,
        pattern: &[Tag(Cc), Tag(Nn), Tag(Nnp), AnyTag(&[Uni, Comp])],
    },
    GrammarRule {
        label: AndCo,
        pattern: &[Tag(Cc), Tag(Nn), Tag(Nnp), Tag(Nnp)],
    },
    GrammarRule {
        label: AndCo,
        pattern: &[Tag(Cc), Tag(Nn), Tag(Nnp), Tag(Nnp), AnyTag(&[Uni, Comp])],
    },
    //
    // #1440  ANDCO: {<CC>+ <NNP> <NNP>+ <UNI|COMP>?}
    // Expanded: CC NNP NNP (covered by #930), CC NNP NNP COMP/UNI, CC NNP NNP NNP COMP/UNI
    GrammarRule {
        label: AndCo,
        pattern: &[Tag(Cc), Tag(Nnp), Tag(Nnp), AnyTag(&[Uni, Comp])],
    },
    GrammarRule {
        label: AndCo,
        pattern: &[Tag(Cc), Tag(Nnp), Tag(Nnp), Tag(Nnp), AnyTag(&[Uni, Comp])],
    },
    //
    // #1450  ANDCO: {<CC>+ <COMPANY|NAME|NAME-EMAIL|NAME-YEAR>+ <UNI|COMP>?}
    // Expanded: CC X, CC X COMP/UNI, CC X X, CC X X COMP/UNI
    // (CC X already covered by #960)
    GrammarRule {
        label: AndCo,
        pattern: &[
            Tag(Cc),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
            AnyTag(&[Uni, Comp]),
        ],
    },
    GrammarRule {
        label: AndCo,
        pattern: &[
            Tag(Cc),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
        ],
    },
    GrammarRule {
        label: AndCo,
        pattern: &[
            Tag(Cc),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
            AnyTag(&[Uni, Comp]),
        ],
    },
    // =========================================================================
    // COMPANY RULES (Python lines 2429–2540, 2600–2620, 2700–2840, 2854–2857)
    // =========================================================================
    //
    // #81  COMP: {<COMP> <COMP>+}
    // COMP is a PosTag, not a TreeLabel. The parser handles COMP merging.
    // We skip this rule; the parser will merge adjacent COMP tokens.
    //
    // #83  COMPANY: {<COMP> <NN> <NNP> <NNP> <COMP> <NNP> <COMP>}
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Comp),
            Tag(Nn),
            Tag(Nnp),
            Tag(Nnp),
            Tag(Comp),
            Tag(Nnp),
            Tag(Comp),
        ],
    },
    //
    // #82  COMPANY: {<COMP> <NN> <NNP> <NNP> <COMP>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Comp), Tag(Nn), Tag(Nnp), Tag(Nnp), Tag(Comp)],
    },
    //
    // #1010  COMPANY: {<NNP> <NNP> <VAN> <NNP> <OF> <NNP> <CC> <COMP>}
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nnp),
            Tag(Nnp),
            Tag(Van),
            Tag(Nnp),
            Tag(Of),
            Tag(Nnp),
            Tag(Cc),
            Tag(Comp),
        ],
    },
    //
    // #1280  COMPANY: {<COMP> <DASHCAPS>+}
    // Expanded: COMP DASHCAPS, COMP DASHCAPS DASHCAPS
    GrammarRule {
        label: Company,
        pattern: &[Tag(Comp), Label(DashCaps)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Comp), Label(DashCaps), Label(DashCaps)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Comp), Label(DashCaps), Label(DashCaps), Label(DashCaps)],
    },
    //
    // #1281  COMPANY: {<COMP> <MAINT> <NNP>+}
    // Expanded: COMP MAINT NNP, COMP MAINT NNP NNP
    GrammarRule {
        label: Company,
        pattern: &[Tag(Comp), Tag(Maint), Tag(Nnp)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Comp), Tag(Maint), Tag(Nnp), Tag(Nnp)],
    },
    //
    // #130  COMPANY: {<BY>? <NN> <NNP> <OF> <NN> <UNI> <OF> <COMPANY|NAME|NAME-EMAIL> <COMP>?}
    // Expanded (most common forms):
    //   NN NNP OF NN UNI OF COMPANY/NAME/NAME-EMAIL
    //   NN NNP OF NN UNI OF COMPANY/NAME/NAME-EMAIL COMP
    //   BY NN NNP OF NN UNI OF COMPANY/NAME/NAME-EMAIL
    //   BY NN NNP OF NN UNI OF COMPANY/NAME/NAME-EMAIL COMP
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nn),
            Tag(Nnp),
            Tag(Of),
            Tag(Nn),
            Tag(Uni),
            Tag(Of),
            AnyLabel(&[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nn),
            Tag(Nnp),
            Tag(Of),
            Tag(Nn),
            Tag(Uni),
            Tag(Of),
            AnyLabel(&[Company, Name, NameEmail]),
            Tag(Comp),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(By),
            Tag(Nn),
            Tag(Nnp),
            Tag(Of),
            Tag(Nn),
            Tag(Uni),
            Tag(Of),
            AnyLabel(&[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(By),
            Tag(Nn),
            Tag(Nnp),
            Tag(Of),
            Tag(Nn),
            Tag(Uni),
            Tag(Of),
            AnyLabel(&[Company, Name, NameEmail]),
            Tag(Comp),
        ],
    },
    //
    // #135  COMPANY: {<NN|NNP> <NNP> <COMP> <COMP>}
    GrammarRule {
        label: Company,
        pattern: &[AnyTag(&[Nn, Nnp]), Tag(Nnp), Tag(Comp), Tag(Comp)],
    },
    //
    // #136  COMPANY: {<NNP>+ <COMP> <EMAIL>}
    // Expanded: NNP COMP EMAIL, NNP NNP COMP EMAIL
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(Comp), Tag(Email)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(Nnp), Tag(Comp), Tag(Email)],
    },
    //
    // #140  COMPANY: {<COMP> <NN> <NNP> <COMP> <NNP>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Comp), Tag(Nn), Tag(Nnp), Tag(Comp), Tag(Nnp)],
    },
    //
    // #144  COMPANY: {<COMP> <COMP> <NNP> <NNP>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Comp), Tag(Comp), Tag(Nnp), Tag(Nnp)],
    },
    //
    // #145  COMPANY: {<COMP> <COMP> <NNP>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Comp), Tag(Comp), Tag(Nnp)],
    },
    //
    // #170  COMPANY: {<COMP> <CD|CDS> <COMP>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Comp), AnyTag(&[Cd, Cds]), Tag(Comp)],
    },
    //
    // #180  COMPANY: {<NNP> <IN> <NN> <NNP> <NNP>+ <COMP>?}
    // Expanded: NNP IN NN NNP NNP, NNP IN NN NNP NNP COMP,
    //           NNP IN NN NNP NNP NNP, NNP IN NN NNP NNP NNP COMP
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(In), Tag(Nn), Tag(Nnp), Tag(Nnp)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(In), Tag(Nn), Tag(Nnp), Tag(Nnp), Tag(Comp)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(In), Tag(Nn), Tag(Nnp), Tag(Nnp), Tag(Nnp)],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nnp),
            Tag(In),
            Tag(Nn),
            Tag(Nnp),
            Tag(Nnp),
            Tag(Nnp),
            Tag(Comp),
        ],
    },
    //
    // #190  COMPANY: {<NNP> <NNP> <CC> <NNP> <COMP> <NNP> <CAPS>}
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nnp),
            Tag(Nnp),
            Tag(Cc),
            Tag(Nnp),
            Tag(Comp),
            Tag(Nnp),
            Tag(Caps),
        ],
    },
    //
    // #200  COMPANY: {<NNP> <CC> <NNP> <COMP> <NNP>*}
    // Expanded: NNP CC NNP COMP, NNP CC NNP COMP NNP
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(Cc), Tag(Nnp), Tag(Comp)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(Cc), Tag(Nnp), Tag(Comp), Tag(Nnp)],
    },
    //
    // #205  COMPANY: {<NN>? <NN> <NNP> <COMP>}
    // Expanded: NN NNP COMP, NN NN NNP COMP
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nn), Tag(Nnp), Tag(Comp)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nn), Tag(Nn), Tag(Nnp), Tag(Comp)],
    },
    //
    // #206  NAME: {<NNP> <NNP> <COMP> <CONTRIBUTORS> <URL|URL2>}
    GrammarRule {
        label: Name,
        pattern: &[
            Tag(Nnp),
            Tag(Nnp),
            Tag(Comp),
            Tag(Contributors),
            AnyTag(&[Url, Url2]),
        ],
    },
    //
    // #207  COMPANY: {<NNP> <NN> <NNP> <NNP> <COMP>+}
    // Expanded: NNP NN NNP NNP COMP, NNP NN NNP NNP COMP COMP
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(Nn), Tag(Nnp), Tag(Nnp), Tag(Comp)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(Nn), Tag(Nnp), Tag(Nnp), Tag(Comp), Tag(Comp)],
    },
    //
    // #208  COMPANY: {<NNP> <COMP|COMPANY> <OF> <NNP>+}
    // Expanded: NNP COMP/COMPANY OF NNP, NNP COMP/COMPANY OF NNP NNP
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nnp),
            AnyTagOrLabel(&[Comp], &[Company]),
            Tag(Of),
            Tag(Nnp),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nnp),
            AnyTagOrLabel(&[Comp], &[Company]),
            Tag(Of),
            Tag(Nnp),
            Tag(Nnp),
        ],
    },
    //
    // #210  COMPANY: {<NNP|CAPS>+ <COMP|COMPANY>+}
    // Expanded: NNP/CAPS COMP/COMPANY, NNP/CAPS NNP/CAPS COMP/COMPANY,
    //           NNP/CAPS COMP/COMPANY COMP/COMPANY
    GrammarRule {
        label: Company,
        pattern: &[AnyTag(&[Nnp, Caps]), AnyTagOrLabel(&[Comp], &[Company])],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            AnyTag(&[Nnp, Caps]),
            AnyTag(&[Nnp, Caps]),
            AnyTagOrLabel(&[Comp], &[Company]),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            AnyTag(&[Nnp, Caps]),
            AnyTagOrLabel(&[Comp], &[Company]),
            AnyTagOrLabel(&[Comp], &[Company]),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            AnyTag(&[Nnp, Caps]),
            AnyTag(&[Nnp, Caps]),
            AnyTagOrLabel(&[Comp], &[Company]),
            AnyTagOrLabel(&[Comp], &[Company]),
        ],
    },
    //
    // #210.5  Rust-specific: handle comma-separated company suffixes.
    // In Python, "Name," is a single NNP/NN token, so "Name, Inc." tokenizes
    // as NNP COMP → matched by #210/#820. In Rust the comma is split out,
    // giving NNP CC COMP / NN CC COMP. These rules bridge that gap.
    // Placed here (after #210) so they fire before simpler NNP/NN rules
    // consume the leading token.
    //
    // NNP CC COMP/COMPANY → COMPANY  (e.g. "Name, Inc.")
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(Cc), AnyTagOrLabel(&[Comp], &[Company])],
    },
    // NNP NNP CC COMP/COMPANY → COMPANY  (e.g. "Foo Name, Inc.")
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nnp),
            Tag(Nnp),
            Tag(Cc),
            AnyTagOrLabel(&[Comp], &[Company]),
        ],
    },
    // NN CC COMP/COMPANY → COMPANY  (e.g. "name, inc.")
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nn), Tag(Cc), AnyTagOrLabel(&[Comp], &[Company])],
    },
    // NN NN CC COMP/COMPANY → COMPANY  (e.g. "company name, inc.")
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nn),
            Tag(Nn),
            Tag(Cc),
            AnyTagOrLabel(&[Comp], &[Company]),
        ],
    },
    //
    // #211  COMPANY: {<UNI> <OF> <COMPANY> <CAPS>?}
    // Expanded: UNI OF COMPANY, UNI OF COMPANY CAPS
    GrammarRule {
        label: Company,
        pattern: &[Tag(Uni), Tag(Of), Label(Company), Tag(Caps)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Uni), Tag(Of), Label(Company)],
    },
    //
    // #220  COMPANY: {<UNI|NNP> <VAN|OF> <NNP>+ <UNI>?}
    // Expanded: UNI/NNP VAN/OF NNP, UNI/NNP VAN/OF NNP UNI,
    //           UNI/NNP VAN/OF NNP NNP, UNI/NNP VAN/OF NNP NNP UNI
    GrammarRule {
        label: Company,
        pattern: &[AnyTag(&[Uni, Nnp]), AnyTag(&[Van, Of]), Tag(Nnp)],
    },
    GrammarRule {
        label: Company,
        pattern: &[AnyTag(&[Uni, Nnp]), AnyTag(&[Van, Of]), Tag(Nnp), Tag(Uni)],
    },
    GrammarRule {
        label: Company,
        pattern: &[AnyTag(&[Uni, Nnp]), AnyTag(&[Van, Of]), Tag(Nnp), Tag(Nnp)],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            AnyTag(&[Uni, Nnp]),
            AnyTag(&[Van, Of]),
            Tag(Nnp),
            Tag(Nnp),
            Tag(Uni),
        ],
    },
    //
    // #230  COMPANY: {<NNP>+ <UNI>}
    // Expanded: NNP UNI, NNP NNP UNI
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(Uni)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(Nnp), Tag(Uni)],
    },
    //
    // #240  COMPANY: {<UNI> <OF> <NN|NNP>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Uni), Tag(Of), AnyTag(&[Nn, Nnp])],
    },
    //
    // #250  COMPANY: {<COMPANY> <CC> <COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Cc), Label(Company)],
    },
    //
    // #251  COMPANY: {<COMPANY> <COMPANY> <CAPS>}
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Label(Company), Tag(Caps)],
    },
    //
    // #252  COMPANY: {<UNI> <OF> <COMP|COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Uni), Tag(Of), AnyTagOrLabel(&[Comp], &[Company])],
    },
    //
    // #253  COMPANY: {<CAPS> <NN> <COMP> <NN> <NNP>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Caps), Tag(Nn), Tag(Comp), Tag(Nn), Tag(Nnp)],
    },
    //
    // #255  COMPANY: {<CAPS> <NN> <COMP>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Caps), Tag(Nn), Tag(Comp)],
    },
    //
    // #256  COMPANY: {<COMP> <CONTRIBUTORS>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Comp), Tag(Contributors)],
    },
    //
    // #259  COMPANY: {<NNP> <JUNK> <NN> <COMP>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(Junk), Tag(Nn), Tag(Comp)],
    },
    //
    // #260  COMPANY: {<LINUX>? <COMP>+}
    // Expanded: COMP, COMP COMP, LINUX COMP, LINUX COMP COMP
    GrammarRule {
        label: Company,
        pattern: &[Tag(Comp)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Comp), Tag(Comp)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Linux), Tag(Comp)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Linux), Tag(Comp), Tag(Comp)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Junk), Tag(Cc), Label(Company)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Junk), Label(Company)],
    },
    //
    // #265  COMPANY: {<COMPANY> <CC> <NN> <COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Cc), Tag(Nn), Label(Company)],
    },
    //
    // #270  COMPANY: {<COMPANY> <CC> <NNP>+}
    // Expanded: COMPANY CC NNP, COMPANY CC NNP NNP
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Cc), Tag(Nnp)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Cc), Tag(Nnp), Tag(Nnp)],
    },
    //
    // #290  COMPANY: {<COMPANY> <DASH> <NNP|NN> <EMAIL>?}
    // Expanded: COMPANY DASH NNP/NN, COMPANY DASH NNP/NN EMAIL
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Dash), AnyTag(&[Nnp, Nn])],
    },
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Dash), AnyTag(&[Nnp, Nn]), Tag(Email)],
    },
    //
    // #510  COMPANY: {<NNP> <IN> <NN>? <COMPANY>}
    // Expanded: NNP IN COMPANY, NNP IN NN COMPANY
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(In), Label(Company)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(In), Tag(Nn), Label(Company)],
    },
    //
    // #529  COMPANY: {<COMPANY> <OF> <COMPANY> <NAME>}
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Of), Label(Company), Label(Name)],
    },
    //
    // #5391  COMPANY: {<COMPANY> <NNP> <OF> <COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Nnp), Tag(Of), Label(Company)],
    },
    //
    // #5292  COMPANY: {<COMPANY> <CAPS> <DASH> <COMPANY> <NAME>}
    GrammarRule {
        label: Company,
        pattern: &[
            Label(Company),
            Tag(Caps),
            Tag(Dash),
            Label(Company),
            Label(Name),
        ],
    },
    //
    // #5293  COMPANY: {<COMPANY> <OF> <NNP> <CC> <COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Of), Tag(Nnp), Tag(Cc), Label(Company)],
    },
    //
    // #52934  COMPANY: {<COMPANY> <NNP> <VAN> <COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Nnp), Tag(Van), Label(Company)],
    },
    //
    // #770  COMPANY: {<NAME|NAME-EMAIL|NAME-YEAR|NNP>+ <OF> <NN>? <COMPANY|COMP> <NNP>?}
    // Expanded (most common forms):
    //   NAME/... OF COMPANY/COMP
    //   NAME/... OF COMPANY/COMP NNP
    //   NAME/... OF NN COMPANY/COMP
    //   NAME/... OF NN COMPANY/COMP NNP
    //   NNP OF COMPANY/COMP
    //   NNP OF COMPANY/COMP NNP
    //   NNP OF NN COMPANY/COMP
    GrammarRule {
        label: Company,
        pattern: &[
            AnyLabel(&[Name, NameEmail, NameYear]),
            Tag(Of),
            AnyTagOrLabel(&[Comp], &[Company]),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            AnyLabel(&[Name, NameEmail, NameYear]),
            Tag(Of),
            AnyTagOrLabel(&[Comp], &[Company]),
            Tag(Nnp),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            AnyLabel(&[Name, NameEmail, NameYear]),
            Tag(Of),
            Tag(Nn),
            AnyTagOrLabel(&[Comp], &[Company]),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            AnyLabel(&[Name, NameEmail, NameYear]),
            Tag(Of),
            Tag(Nn),
            AnyTagOrLabel(&[Comp], &[Company]),
            Tag(Nnp),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(Of), AnyTagOrLabel(&[Comp], &[Company])],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nnp),
            Tag(Of),
            AnyTagOrLabel(&[Comp], &[Company]),
            Tag(Nnp),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nnp),
            Tag(Of),
            Tag(Nn),
            AnyTagOrLabel(&[Comp], &[Company]),
        ],
    },
    //
    // #780  COMPANY: {<NNP> <COMP|COMPANY> <COMP|COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nnp),
            AnyTagOrLabel(&[Comp], &[Company]),
            AnyTagOrLabel(&[Comp], &[Company]),
        ],
    },
    //
    // #790  COMPANY: {<NN>? <COMPANY|NAME|NAME-EMAIL> <CC> <COMPANY|NAME|NAME-EMAIL>}
    // Expanded: X CC Y, NN X CC Y
    GrammarRule {
        label: Company,
        pattern: &[
            AnyLabel(&[Company, Name, NameEmail]),
            Tag(Cc),
            AnyLabel(&[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nn),
            AnyLabel(&[Company, Name, NameEmail]),
            Tag(Cc),
            AnyLabel(&[Company, Name, NameEmail]),
        ],
    },
    //
    // #800  COMPANY: {<COMP|COMPANY|NNP> <NN> <COMPANY> <NNP>+}
    // Expanded: COMP/COMPANY/NNP NN COMPANY NNP, COMP/COMPANY/NNP NN COMPANY NNP NNP
    GrammarRule {
        label: Company,
        pattern: &[
            AnyTagOrLabel(&[Comp, Nnp], &[Company]),
            Tag(Nn),
            Label(Company),
            Tag(Nnp),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            AnyTagOrLabel(&[Comp, Nnp], &[Company]),
            Tag(Nn),
            Label(Company),
            Tag(Nnp),
            Tag(Nnp),
        ],
    },
    //
    // #805  COMPANY: {<BY> <NN> <COMPANY> <OF> <NNP> <CC> <COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(By),
            Tag(Nn),
            Label(Company),
            Tag(Of),
            Tag(Nnp),
            Tag(Cc),
            Label(Company),
        ],
    },
    //
    // #810  COMPANY: {<COMPANY> <CC> <AUTH|CONTRIBUTORS|AUTHS>}
    GrammarRule {
        label: Company,
        pattern: &[
            Label(Company),
            Tag(Cc),
            AnyTag(&[Auth, Contributors, Auths]),
        ],
    },
    //
    // #815  COMPANY: {<NN> <COMP|COMPANY> <OF> <MAINT>}
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nn),
            AnyTagOrLabel(&[Comp], &[Company]),
            Tag(Of),
            Tag(Maint),
        ],
    },
    //
    // #820  COMPANY: {<NN> <COMP|COMPANY>+ <AUTHS>?}
    // Expanded: NN COMP/COMPANY, NN COMP/COMPANY AUTHS,
    //           NN COMP/COMPANY COMP/COMPANY, NN COMP/COMPANY COMP/COMPANY AUTHS
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nn), AnyTagOrLabel(&[Comp], &[Company])],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nn), AnyTagOrLabel(&[Comp], &[Company]), Tag(Auths)],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nn),
            AnyTagOrLabel(&[Comp], &[Company]),
            AnyTagOrLabel(&[Comp], &[Company]),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nn),
            AnyTagOrLabel(&[Comp], &[Company]),
            AnyTagOrLabel(&[Comp], &[Company]),
            Tag(Auths),
        ],
    },
    //
    // #830  COMPANY: {<NNP>? <URL|URL2>}
    // Expanded: URL/URL2, NNP URL/URL2
    GrammarRule {
        label: Company,
        pattern: &[AnyTag(&[Url, Url2])],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), AnyTag(&[Url, Url2])],
    },
    //
    // #840  COMPANY: {<COMPANY> <COMP|COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), AnyTagOrLabel(&[Comp], &[Company])],
    },
    //
    // #840.1  COMPANY: {<COMPANY> <OF> <COMP|COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Of), AnyTagOrLabel(&[Comp], &[Company])],
    },
    //
    // #845  COMPANY: {<UNI> <COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Uni), Label(Company)],
    },
    //
    // #900  COMPANY: {<NAME|NAME-EMAIL|NNP>+ <CONTRIBUTORS>}
    // Expanded: NAME/NAME-EMAIL CONTRIBUTORS, NNP CONTRIBUTORS,
    //           NAME/NAME-EMAIL NAME/NAME-EMAIL CONTRIBUTORS, NNP NNP CONTRIBUTORS
    GrammarRule {
        label: Company,
        pattern: &[AnyLabel(&[Name, NameEmail]), Tag(Contributors)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(Contributors)],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            AnyLabel(&[Name, NameEmail]),
            AnyLabel(&[Name, NameEmail]),
            Tag(Contributors),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(Nnp), Tag(Contributors)],
    },
    //
    // #910  COMPANY: {<PN> <COMP|COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Pn), AnyTagOrLabel(&[Comp], &[Company])],
    },
    //
    // #970  COMPANY: {<COMPANY|NAME|NAME-EMAIL|NAME-YEAR> <ANDCO>+}
    // Expanded: X ANDCO, X ANDCO ANDCO
    GrammarRule {
        label: Company,
        pattern: &[
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
            Label(AndCo),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
            Label(AndCo),
            Label(AndCo),
        ],
    },
    //
    // #970 (second)  COMPANY: {<COMPANY|NAME|NAME-EMAIL|NAME-YEAR> <PARENS>? <ANDCO>+}
    // Expanded: X PARENS ANDCO, X PARENS ANDCO ANDCO
    GrammarRule {
        label: Company,
        pattern: &[
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
            Tag(Parens),
            Label(AndCo),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
            Tag(Parens),
            Label(AndCo),
            Label(AndCo),
        ],
    },
    //
    // #1030  COMPANY: {<NNP> <COMPANY> <NN|NNP> <NAME>?}
    // Expanded: NNP COMPANY NN/NNP, NNP COMPANY NN/NNP NAME
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Label(Company), AnyTag(&[Nn, Nnp])],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Label(Company), AnyTag(&[Nn, Nnp]), Label(Name)],
    },
    //
    // #1150  COMPANY: {<COMPANY> <CC> <OTH>}
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Cc), Tag(Oth)],
    },
    //
    // #1160  COMPANY: {<NAME-YEAR> <CC> <OTH>}
    GrammarRule {
        label: Company,
        pattern: &[Label(NameYear), Tag(Cc), Tag(Oth)],
    },
    //
    // #1190  COMPANY: {<NNP> <COMPANY> <CC> <COMP>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Label(Company), Tag(Cc), Tag(Comp)],
    },
    //
    // #1220  COMPANY: {<NNP> <COMPANY> <NAME>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Label(Company), Label(Name)],
    },
    //
    // #1250  COMPANY: {<NN> <NN> <NN>? <COMPANY>}
    // Expanded: NN NN COMPANY, NN NN NN COMPANY
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nn), Tag(Nn), Label(Company)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nn), Tag(Nn), Tag(Nn), Label(Company)],
    },
    //
    // #1251  COMPANY: {<NN> <NNP> <COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nn), Tag(Nnp), Label(Company)],
    },
    //
    // #1310  COMPANY: {<NNP> <IN> <NN>+ <COMPANY>}
    // Expanded: NNP IN NN COMPANY, NNP IN NN NN COMPANY
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(In), Tag(Nn), Label(Company)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Tag(In), Tag(Nn), Tag(Nn), Label(Company)],
    },
    //
    // #1340  COMPANY: {<OU> <COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Ou), Label(Company)],
    },
    //
    // #1370  COMPANY: {<CAPS>+ <COMPANY>}
    // Expanded: CAPS COMPANY, CAPS CAPS COMPANY
    GrammarRule {
        label: Company,
        pattern: &[Tag(Caps), Label(Company)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Tag(Caps), Tag(Caps), Label(Company)],
    },
    //
    // #1400  COMPANY: {<COMPANY> <EMAIL>+}
    // Expanded: COMPANY EMAIL, COMPANY EMAIL EMAIL
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Email)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Email), Tag(Email)],
    },
    //
    // #1420  COMPANY: {<BY> <NN>+ <COMP|COMPANY>}
    // Expanded: BY NN COMP/COMPANY, BY NN NN COMP/COMPANY
    GrammarRule {
        label: Company,
        pattern: &[Tag(By), Tag(Nn), AnyTagOrLabel(&[Comp], &[Company])],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(By),
            Tag(Nn),
            Tag(Nn),
            AnyTagOrLabel(&[Comp], &[Company]),
        ],
    },
    //
    // #1422  COMPANY: {<NN> <NNP> <OF> <NN> <UNI> <OF> <COMPANY>+}
    // Expanded: NN NNP OF NN UNI OF COMPANY, NN NNP OF NN UNI OF COMPANY COMPANY
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nn),
            Tag(Nnp),
            Tag(Of),
            Tag(Nn),
            Tag(Uni),
            Tag(Of),
            Label(Company),
        ],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            Tag(Nn),
            Tag(Nnp),
            Tag(Of),
            Tag(Nn),
            Tag(Uni),
            Tag(Of),
            Label(Company),
            Label(Company),
        ],
    },
    //
    // #1427  COMPANY: {<UNI> <UNI> <NNP>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Uni), Tag(Uni), Tag(Nnp)],
    },
    //
    // #1460  COMPANY: {<COMPANY|NAME|NAME-EMAIL|NAME-YEAR> <ANDCO>+}
    // Already covered by #970
    //
    // #1480  COMPANY: {<COMPANY> <COMPANY>+}
    // Expanded: COMPANY COMPANY, COMPANY COMPANY COMPANY
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Label(Company)],
    },
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Label(Company), Label(Company)],
    },
    //
    // #1490  COMPANY: {<CC> <IN> <COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Cc), Tag(In), Label(Company)],
    },
    //
    // #1411  COMPANY: {<COMPANY> <CC> <NN> <CONTRIBUTORS>}
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Cc), Tag(Nn), Tag(Contributors)],
    },
    //
    // #1413  COMPANY: {<NAME> <CC> <NN> <COMPANY>+}
    // Expanded: NAME CC NN COMPANY, NAME CC NN COMPANY COMPANY
    GrammarRule {
        label: Company,
        pattern: &[Label(Name), Tag(Cc), Tag(Nn), Label(Company)],
    },
    GrammarRule {
        label: Company,
        pattern: &[
            Label(Name),
            Tag(Cc),
            Tag(Nn),
            Label(Company),
            Label(Company),
        ],
    },
    //
    // #1414  COMPANY: {<NN> <COMPANY> <CC> <NN> <COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nn), Label(Company), Tag(Cc), Tag(Nn), Label(Company)],
    },
    //
    // #1415  COMPANY: {<BY> <COMPANY> <OF> <COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(By), Label(Company), Tag(Of), Label(Company)],
    },
    //
    // #1416  COMPANY: {<NNP> <COMPANY> <OF> <COMPANY> <NNP>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nnp), Label(Company), Tag(Of), Label(Company), Tag(Nnp)],
    },
    //
    // #19602  COMPANY: {<NN> <CAPS> <NN> <MAINT> <COMPANY>}
    GrammarRule {
        label: Company,
        pattern: &[Tag(Nn), Tag(Caps), Tag(Nn), Tag(Maint), Label(Company)],
    },
    //
    // #19603  COMPANY: {<COMPANY> <MAINT>}
    GrammarRule {
        label: Company,
        pattern: &[Label(Company), Tag(Maint)],
    },
    // =========================================================================
    // INITIALDEV (Python line 2863)
    // =========================================================================
    //
    // #19663  INITIALDEV: {<BY>? <NN> <NN> <MAINT>}
    // Expanded: NN NN MAINT, BY NN NN MAINT
    GrammarRule {
        label: InitialDev,
        pattern: &[Tag(Nn), Tag(Nn), Tag(Maint)],
    },
    GrammarRule {
        label: InitialDev,
        pattern: &[Tag(By), Tag(Nn), Tag(Nn), Tag(Maint)],
    },
    // =========================================================================
    // COPYRIGHT RULES (Python lines 2870–3400)
    // =========================================================================
    // #1510
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(Name), Tag(Copy), Label(YrRange)],
    },
    // #1530 expanded
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Email)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(By), Tag(Email)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(Email)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(By), Tag(Email)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), AnyLabel(&[Company, Name]), Tag(Email)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), AnyLabel(&[Company, Name]), Tag(Email)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), AnyLabel(&[Company, Name]), Tag(By), Tag(Email)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(YrRange), Tag(Email)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(YrRange), Tag(By), Tag(Email)],
    },
    // #1550
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            AnyLabel(&[Name, NameEmail, NameYear]),
            Tag(Caps),
            Label(YrRange),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            AnyLabel(&[Name, NameEmail, NameYear]),
            Tag(Caps),
            Label(YrRange),
        ],
    },
    // #1560
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Label(NameYear)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(Nn), Label(NameYear)],
    },
    // #1562
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(NameYear),
            Tag(In),
            Tag(Nn),
            Tag(Nn),
            Tag(Nnp),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(NameYear),
            Tag(In),
            Tag(Nn),
            Tag(Nn),
            Tag(Nnp),
        ],
    },
    // #1565
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Nnp), Tag(Copy), Label(NameYear), Label(Company)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Nnp),
            Tag(Copy),
            Tag(Copy),
            Label(NameYear),
            Label(Company),
        ],
    },
    // #1566
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(NameYear), AnyTag(&[Nn, Nnp]), Tag(Auths)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(NameYear),
            AnyTag(&[Nn, Nnp]),
            Tag(Auths),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(NameYear),
            AnyTag(&[Nn, Nnp]),
            AnyTag(&[Nn, Nnp]),
            Tag(Auths),
        ],
    },
    // #1579992
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(Name), Tag(Cc), Tag(Nn), Label(YrRange)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(Name),
            Tag(Cc),
            Tag(Nn),
            Label(YrRange),
        ],
    },
    // #1579998
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(NameYear), AnyTag(&[Nn, Dash]), Tag(Email)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(NameYear),
            AnyTag(&[Nn, Dash]),
            Tag(Email),
        ],
    },
    // #83005
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(NameYear),
            Tag(Nn),
            Tag(Caps),
            Tag(Nn),
            Tag(Of),
            Label(Company),
            Label(Name),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(NameYear),
            Tag(Nn),
            Tag(Caps),
            Tag(Nn),
            Tag(Of),
            Label(Company),
            Label(Name),
        ],
    },
    // #157999
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            AnyLabel(&[Name, NameEmail, NameYear]),
            Label(YrRange),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            AnyLabel(&[Name, NameEmail, NameYear]),
            Label(YrRange),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            AnyLabel(&[Name, NameEmail, NameYear]),
            AnyLabel(&[Name, NameEmail, NameYear]),
            Label(YrRange),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            AnyLabel(&[Name, NameEmail, NameYear]),
            AnyLabel(&[Name, NameEmail, NameYear]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            AnyLabel(&[Name, NameEmail, NameYear]),
            AnyLabel(&[Name, NameEmail, NameYear]),
        ],
    },
    // #157999-name
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Tag(Nn), Tag(Uni), Label(Name)],
    },
    // #1590
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), AnyTag(&[Caps, Nnp]), Tag(Cc), Tag(Nn), Tag(Copy)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            AnyTag(&[Caps, Nnp]),
            Tag(Cc),
            Tag(Nn),
            Tag(Copy),
            Label(YrRange),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            AnyTag(&[Caps, Nnp]),
            AnyTag(&[Caps, Nnp]),
            Tag(Cc),
            Tag(Nn),
            Tag(Copy),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            AnyTag(&[Caps, Nnp]),
            Tag(Cc),
            Tag(Nn),
            Tag(Copy),
        ],
    },
    // #1610
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), AnyLabel(&[Company, Name, NameEmail])],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), AnyLabel(&[Company, Name, NameEmail])],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            AnyTag(&[By, To]),
            AnyLabel(&[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            AnyTag(&[By, To]),
            AnyLabel(&[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            AnyLabel(&[Company, Name, NameEmail]),
            AnyLabel(&[Company, Name, NameEmail]),
            Label(YrRange),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            AnyTag(&[By, To]),
            AnyLabel(&[Company, Name, NameEmail]),
            Label(YrRange),
        ],
    },
    // #1630 expanded
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            AnyTagOrLabel(&[Nnp], &[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            Tag(By),
            AnyTagOrLabel(&[Nnp], &[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            Tag(Nn),
            AnyTagOrLabel(&[Nnp], &[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            Tag(By),
            Tag(Nn),
            AnyTagOrLabel(&[Nnp], &[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            AnyTagOrLabel(&[Nnp], &[Company, Name, NameEmail]),
            Tag(Email),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            AnyTagOrLabel(&[Nnp], &[Company, Name, NameEmail]),
            AnyTag(&[AuthDot, Maint]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            AnyTagOrLabel(&[Nnp], &[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(By),
            AnyTagOrLabel(&[Nnp], &[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Nn),
            AnyTagOrLabel(&[Nnp], &[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Nnp),
            Tag(Copy),
            Label(YrRange),
            AnyTagOrLabel(&[Nnp], &[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            AnyTagOrLabel(&[Nnp], &[Company, Name, NameEmail]),
            AnyTagOrLabel(&[Nnp], &[Company, Name, NameEmail]),
        ],
    },
    // #1650
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Label(Name), Label(YrRange)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(Nn), Label(Name), Label(YrRange)],
    },
    // #1670
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(By), AnyLabel(&[Name, NameEmail, NameYear])],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Tag(By),
            AnyLabel(&[Name, NameEmail, NameYear]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(By),
            AnyLabel(&[Name, NameEmail, NameYear]),
            AnyLabel(&[Name, NameEmail, NameYear]),
        ],
    },
    // #1690
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(Comp)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(Comp), Tag(Comp)],
    },
    // #1802
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(Mit)],
    },
    // #1710
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Tag(Nn),
            AnyLabel(&[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Tag(Nn),
            Tag(Nn),
            AnyLabel(&[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Tag(Nn),
            AnyLabel(&[Company, Name, NameEmail]),
            AnyLabel(&[Company, Name, NameEmail]),
        ],
    },
    // #1711
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Tag(Nn),
            Tag(Nnp),
            Tag(Nn),
            Label(Company),
        ],
    },
    // #1730/#1750
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Tag(Comp)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Tag(Comp), Label(YrRange)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Tag(Nn), Tag(Comp)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Tag(Nn), Tag(Comp), Label(YrRange)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(Nn), Tag(Comp)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(Nn), Tag(Comp), Label(YrRange)],
    },
    // #1760
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Label(Company)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Label(Company), Label(YrRange)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Tag(Nn), Label(Company)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Tag(Nn), Label(Company), Label(YrRange)],
    },
    // #1780
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            AnyTagOrLabel(&[Nnp], &[YrRange]),
            AnyTagOrLabel(&[Nnp], &[YrRange, Name]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            AnyTagOrLabel(&[Nnp], &[YrRange]),
            AnyTag(&[Caps, By]),
            AnyTagOrLabel(&[Nnp], &[YrRange, Name]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            AnyTagOrLabel(&[Nnp], &[YrRange]),
            AnyTagOrLabel(&[Nnp], &[YrRange, Name]),
            AnyTagOrLabel(&[Nnp], &[YrRange, Name]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            AnyTagOrLabel(&[Nnp], &[YrRange]),
            AnyTagOrLabel(&[Nnp], &[YrRange, Name]),
        ],
    },
    // #1800
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(Nnp)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(Nnp), Tag(Nnp)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(Nnp), Tag(Nnp), Tag(Nnp)],
    },
    // #1801
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Tag(YrPlus),
            AnyLabel(&[Name, NameEmail, NameYear]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Tag(YrPlus),
            AnyLabel(&[Name, NameEmail, NameYear]),
            AnyLabel(&[Name, NameEmail, NameYear]),
        ],
    },
    // =========================================================================
    // COPYRIGHT2 RULES (Python lines 2955–3095)
    // =========================================================================
    // #1830
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Comp),
            Tag(Nnp),
            Tag(Nn),
        ],
    },
    // #1860
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Nn),
            Tag(Nnp),
            Label(AndCo),
        ],
    },
    // #1880
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Nn),
            AnyTag(&[Auth, Contributors, Auths]),
        ],
    },
    // #1920
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Label(YrRange), Tag(Dash), Tag(Nn)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange), Tag(Dash), Tag(Nn)],
    },
    // #1990
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Label(YrRange), Tag(Nn), Tag(Nnp), Tag(Nn)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            AnyLabel(&[Name, NameEmail, NameYear, Company, NameCaps]),
        ],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Nn),
            Tag(Nnp),
            Tag(Nn),
        ],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            AnyLabel(&[Name, NameEmail, NameYear, Company, NameCaps]),
        ],
    },
    // #2020
    GrammarRule {
        label: Copyright2,
        pattern: &[Label(Copyright), Label(Company), Label(YrRange)],
    },
    // #2060
    GrammarRule {
        label: Copyright2,
        pattern: &[Label(Copyright), Label(Company), Label(Company)],
    },
    // #2080
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(By),
            Tag(Nn),
            Tag(Nn),
            Label(Name),
        ],
    },
    // #2090
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            Tag(By),
            Tag(Nn),
            Tag(Nn),
            Label(Name),
        ],
    },
    // #2110
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(By),
            Tag(Nn),
        ],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Tag(Nn),
            Tag(Copy),
            Label(YrRange),
            Tag(By),
            Tag(Nn),
        ],
    },
    // #2115
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange), Tag(By), Tag(Nn)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(By),
            Tag(Nn),
            Tag(Nnp),
        ],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Nn),
            Tag(Copy),
            Label(YrRange),
            Tag(By),
            Tag(Nn),
        ],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Nn),
            Tag(Copy),
            Label(YrRange),
            Tag(By),
            Tag(Nn),
            Tag(Nnp),
        ],
    },
    // #2140
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Tag(Nn), Label(YrRange), Tag(By), Label(Name)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Tag(Nn),
            Label(YrRange),
            Tag(By),
            Label(Name),
        ],
    },
    // #2160
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            Tag(Dash),
            AnyLabel(&[NameEmail, Name]),
        ],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            Tag(Dash),
            Tag(By),
            AnyLabel(&[NameEmail, Name]),
        ],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Dash),
            AnyLabel(&[NameEmail, Name]),
        ],
    },
    // #2180
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Label(YrRange), Tag(Nnp), Label(Name)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange), Tag(Nnp), Label(Name)],
    },
    // #2210
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            Tag(Comp),
            AnyTag(&[Auths, Contributors]),
        ],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Comp),
            AnyTag(&[Auths, Contributors]),
        ],
    },
    // #2230
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Label(YrRange), Tag(Comp)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange), Tag(Comp)],
    },
    // #2240
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange), Tag(MixedCap)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Caps),
            Tag(MixedCap),
        ],
    },
    // #2260
    GrammarRule {
        label: Copyright2,
        pattern: &[Label(Name), Tag(Copy), Label(YrRange)],
    },
    // #2270
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Label(YrRange), Tag(Caps), Tag(Email)],
    },
    // #2271
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange), Tag(Caps), Tag(Caps)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Caps),
            Tag(Caps),
            Tag(Caps),
        ],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Caps),
            Tag(Caps),
            Tag(Caps),
            Tag(Caps),
        ],
    },
    // #2280
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            Tag(Copy),
            AnyLabel(&[Name, NameEmail, NameYear, Company, NameCaps]),
        ],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Label(YrRange)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), AnyTag(&[Nn, Caps]), Label(YrRange)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Tag(Copy), AnyTag(&[Nn, Caps]), Label(YrRange)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Label(YrRange), Tag(Pn)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange), Tag(Pn)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Label(YrRange), Label(YrRange)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange), Label(YrRange)],
    },
    // #2300
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Label(YrRange), Label(Company)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange), Label(Company)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            AnyTag(&[Nn, Caps]),
            Label(Company),
        ],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            AnyTag(&[Nn, Caps]),
            Label(Company),
        ],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Label(YrRange), AnyTag(&[Nn, Caps])],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange), AnyTag(&[Nn, Caps])],
    },
    // #2320
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Label(YrRange), Tag(Dash), Label(Company)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Dash),
            Label(Company),
        ],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            AnyTag(&[Nn, Caps]),
            Tag(Dash),
            Label(Company),
        ],
    },
    // #2340
    GrammarRule {
        label: Copyright2,
        pattern: &[AnyTagOrLabel(&[Nnp], &[Name, Company]), Label(Copyright2)],
    },
    // #22795
    GrammarRule {
        label: Copyright2,
        pattern: &[Tag(Copy), Label(YrRange), Tag(By), AnyTag(&[Nn, Nnp])],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(By),
            AnyTag(&[Nn, Nnp]),
        ],
    },
    // #2010
    GrammarRule {
        label: Copyright2,
        pattern: &[Label(Copyright2), Tag(Junk), Label(Company)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[Label(Copyright2), Tag(Junk), Tag(Cc), Label(Company)],
    },
    GrammarRule {
        label: Copyright2,
        pattern: &[Label(Copyright2), Tag(Junk), Tag(Cc), Tag(Comp)],
    },
    // #2274.2
    GrammarRule {
        label: Copyright2,
        pattern: &[Label(NameCopy), Label(Copyright2)],
    },
    // =========================================================================
    // NAME-COPY, NAME-CAPS (Python lines 3009, 3222)
    // =========================================================================
    // #2272 NAME-COPY
    GrammarRule {
        label: NameCopy,
        pattern: &[Tag(Nnp), Tag(Copy)],
    },
    // #2273 COPYRIGHT2 from NAME-COPY
    GrammarRule {
        label: Copyright2,
        pattern: &[Label(NameCopy), Label(YrRange)],
    },
    // #2530 NAME-CAPS
    GrammarRule {
        label: NameCaps,
        pattern: &[Tag(Caps)],
    },
    GrammarRule {
        label: NameCaps,
        pattern: &[Tag(Caps), Tag(Caps)],
    },
    GrammarRule {
        label: NameCaps,
        pattern: &[Tag(Caps), Tag(Caps), Tag(Caps)],
    },
    // =========================================================================
    // MORE COPYRIGHT RULES (Python lines 3005–3407)
    // =========================================================================
    // #2271.1
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Notice), Label(NameYear)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Notice),
            Label(NameYear),
            Label(AllRightReserved),
        ],
    },
    // #2274
    GrammarRule {
        label: Copyright,
        pattern: &[Label(NameCopy), Tag(Nnp)],
    },
    // #2275
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(YrRange), Label(Copyright)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange), Label(Copyright)],
    },
    // #2276
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Contributors),
            Tag(Oth),
        ],
    },
    // #2277.1 expanded
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Contributors),
            AnyTag(&[Caps, Auths, Auth]),
            Tag(Junk),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Contributors),
            Tag(Nn),
            AnyTag(&[Caps, Auths, Auth]),
            Tag(Junk),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Contributors),
            Tag(In),
            AnyTag(&[Caps, Auths, Auth]),
            Tag(Junk),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Contributors),
            Tag(Nn),
            Tag(In),
            Tag(Nn),
            AnyTag(&[Caps, Auths, Auth]),
            Tag(Junk),
        ],
    },
    // #2278/#2279
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright),
            Tag(Copy),
            Label(YrRange),
            Label(AllRightReserved),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright),
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Label(AllRightReserved),
        ],
    },
    // #22790
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange), Tag(Nn), Tag(Nnp)],
    },
    // #22790.1
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            Tag(Contributors),
            Tag(To),
            Label(Company),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Contributors),
            Tag(To),
            Label(Company),
        ],
    },
    // #22791
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(YrRange), Tag(Contributors)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            Tag(Contributors),
            Label(AllRightReserved),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange), Tag(Contributors)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Contributors),
            Label(AllRightReserved),
        ],
    },
    // #22792
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(YrRange), AnyTag(&[Linux, Nn]), Tag(Nnp)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            AnyTag(&[Linux, Nn]),
            Tag(Nnp),
        ],
    },
    // #22793.1
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange), Label(Company)],
    },
    // #22793.2
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Nn),
            AnyTag(&[Contributors, Commit, Auths, Maint]),
            Tag(Copy),
            Label(YrRange),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Tag(Nn),
            AnyTag(&[Contributors, Commit, Auths, Maint]),
            Tag(Copy),
            Label(YrRange),
        ],
    },
    // #22793.3 expanded
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(YrRange), Tag(Nn)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange), Tag(Nn)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(YrRange), Tag(Nn), Tag(Nn)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            Tag(Nn),
            AnyTag(&[Contributors, Commit, Auths, Maint]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(YrRange), Tag(Nn), Tag(Email)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(YrRange), Tag(Nn), Label(AllRightReserved)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Nn),
            AnyTag(&[Contributors, Commit, Auths, Maint]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Label(YrRange), Tag(Nn), Tag(Nn)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            Tag(Nn),
            AnyTag(&[Contributors, Commit, Auths, Maint]),
            Tag(Email),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            Tag(Nn),
            AnyTag(&[Contributors, Commit, Auths, Maint]),
            Label(AllRightReserved),
        ],
    },
    // #22793.4
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Tag(Email)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Tag(Email), Label(AllRightReserved)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(Nn), Tag(Email)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Tag(Nn),
            Tag(Email),
            Label(AllRightReserved),
        ],
    },
    // #22794
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Company), Label(AllRightReserved), Label(Copyright)],
    },
    // #230020
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(YrRange), Tag(Copy), Tag(Nnp)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(YrRange),
            Tag(Copy),
            AnyLabel(&[Name, NameEmail, NameYear, Company, NameCaps]),
        ],
    },
    // #2280-1
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Caps),
            AnyTag(&[Nn, Linux]),
            Tag(Nnp),
        ],
    },
    // #2280-2
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(NameEmail),
            Label(YrRange),
            Tag(Auth2),
            Tag(By),
            Label(NameEmail),
            Tag(Copy),
            Label(YrRange),
        ],
    },
    // #2280-3
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(YrRange),
            Tag(Auth),
            Label(NameEmail),
        ],
    },
    // #2280-4
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Tag(By), AnyLabel(&[NameYear, NameEmail])],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright2),
            Tag(By),
            AnyLabel(&[NameYear, NameEmail]),
            AnyLabel(&[NameYear, NameEmail]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright2),
            Tag(By),
            AnyLabel(&[NameYear, NameEmail]),
            Tag(By),
            AnyLabel(&[NameYear, NameEmail]),
        ],
    },
    // #2280.123
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Tag(Maint), Tag(Of), Label(Company)],
    },
    // #2862
    GrammarRule {
        label: Copyright,
        pattern: &[
            AnyLabel(&[Copyright, Copyright2]),
            AnyTag(&[Nn, Nnp, Contributors]),
            Label(AllRightReserved),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            AnyLabel(&[Copyright, Copyright2]),
            AnyTag(&[Nn, Nnp, Contributors]),
            AnyTag(&[Nn, Nnp, Contributors]),
            Label(AllRightReserved),
        ],
    },
    // #2360
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Tag(Nn), Label(Company)],
    },
    // #2380
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(By), Tag(Nn), Label(Company)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(By), Tag(Nn), Label(Company)],
    },
    // #2400
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Company), Tag(Nn), Label(Name), Label(Copyright2)],
    },
    // #2410
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Tag(Comp), Label(Company)],
    },
    // #2430
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Tag(Nnp), Tag(Cc), Label(Company)],
    },
    // #2860
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), AnyLabel(&[Name, NameEmail, NameYear])],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Label(Copyright)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright2),
            AnyLabel(&[Name, NameEmail, NameYear]),
            AnyLabel(&[Name, NameEmail, NameYear]),
        ],
    },
    // #2861
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright2),
            Label(AllRightReserved),
            Tag(By),
            Label(Company),
        ],
    },
    // #2400 (second)
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Label(Name)],
    },
    // #2460
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Nnp), Tag(Nn), Tag(Copy), Tag(Nnp)],
    },
    // #2470
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Nnp), Tag(Copy), Tag(Nnp)],
    },
    // #2500
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Nn),
            Tag(Copy),
            Label(YrRange),
            Label(Company),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Nn),
            Tag(Copy),
            Label(YrRange),
            Label(Company),
            Label(Company),
        ],
    },
    // #2580
    GrammarRule {
        label: Copyright,
        pattern: &[AnyLabel(&[Copyright, Copyright2]), Label(Company)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            AnyLabel(&[Copyright, Copyright2]),
            Label(Company),
            Label(Company),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            AnyLabel(&[Copyright, Copyright2]),
            Label(Company),
            Label(Name),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            AnyLabel(&[Copyright, Copyright2]),
            Label(Company),
            Label(Company),
            Label(Name),
        ],
    },
    // #2590
    GrammarRule {
        label: Copyright,
        pattern: &[Label(AndCo), Label(Copyright2)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Label(AndCo), Tag(Nn), Label(Copyright2)],
    },
    // #2609
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Portions), Label(Copyright), Tag(Nn), Tag(Nnp)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Portions),
            Label(Copyright),
            Tag(Nn),
            Tag(Nnp),
            Label(YrRange),
        ],
    },
    // #2610
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Portions), AnyLabel(&[Copyright, Copyright2])],
    },
    // #2620
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Notice), Label(Company), Label(YrRange)],
    },
    // #2625
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Label(AndCo)],
    },
    // #2630
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Pn), Label(YrRange), Tag(By), Label(Company)],
    },
    // #2632
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Tag(MixedCap)],
    },
    // #2634
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Tag(Dash), Label(Name)],
    },
    // #2635
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Tag(Nn), Label(Name)],
    },
    // #2636
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Tag(Email)],
    },
    // #2637
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Tag(Caps), Label(NameEmail)],
    },
    // #26381
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Tag(Nnp), Label(Company)],
    },
    // #2639
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Tag(Dash), Label(Company)],
    },
    // #1565 (second)
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Tag(Nnp), Label(NameYear)],
    },
    // #1566 (second)
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(YrPlus), Label(Copyright)],
    },
    // #2000
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Tag(Email)],
    },
    // #2001
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), AnyLabel(&[Name, NameYear])],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright),
            AnyLabel(&[Name, NameYear]),
            AnyLabel(&[Name, NameYear]),
        ],
    },
    // #2002
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(By), AnyTag(&[Nnp, Caps])],
    },
    // #2003
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Tag(Of), Label(Company)],
    },
    // #2004
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(YrRange), Tag(Pn)],
    },
    // #2004.1
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(Caps), Tag(Nn), Tag(Email)],
    },
    // #2005
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(Caps)],
    },
    // #2006
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(By), Label(Company)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(By), Tag(Nn), Label(Company)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(By), Tag(Nnp), Label(Company)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(By), Tag(Nn), Tag(Nnp), Label(Company)],
    },
    // #2007
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(YrRange), Tag(By), Tag(Nn), Label(Name)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Pn),
            Label(YrRange),
            Tag(By),
            Tag(Nn),
            Label(Name),
        ],
    },
    // #2008
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright2),
            AnyTagOrLabel(&[Caps], &[Company]),
            AnyTag(&[Nn, Linux]),
            Label(Company),
        ],
    },
    // #2009.1
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright2),
            Tag(Caps),
            AnyTag(&[Cd, Cds]),
            Label(Company),
            Label(Name),
        ],
    },
    // #2009
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Tag(Caps)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Tag(Caps), Tag(Caps)],
    },
    // W3C pattern: "(c) YEAR (W3C) MIT, ERCIM, Keio University"
    // After grammar reductions: Copyright2 NameCaps Mit AndCo AndCo
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright2),
            Label(NameCaps),
            Tag(Mit),
            Label(AndCo),
            Label(AndCo),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Label(NameCaps), Tag(Mit), Label(AndCo)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Label(NameCaps), Tag(Mit)],
    },
    // #2274.1
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Pn), Label(YrRange), Label(Company)],
    },
    // #2276 (second)
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Tag(Uni), Tag(Of), Tag(Caps)],
    },
    // #25501
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(NameCaps), Label(NameYear)],
    },
    // #2560
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Nn),
            Tag(Copy),
            AnyLabel(&[Copyright, NameCaps]),
        ],
    },
    // #2561
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Tag(By), Label(NameCaps)],
    },
    // #2562
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Tag(Nn), Tag(Nnp)],
    },
    // #2563
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Label(AndCo)],
    },
    // #2840
    GrammarRule {
        label: Copyright,
        pattern: &[Label(NameEmail), Label(Copyright2)],
    },
    // #26371
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Pn)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Of), Tag(Pn)],
    },
    // #3000
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright2),
            Tag(Copy),
            Tag(Nn),
            Tag(Nnp),
            Label(AllRightReserved),
        ],
    },
    // #3000 (second)
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright),
            Tag(Of),
            Label(Company),
            Label(Name),
            Label(Name),
            Label(Company),
        ],
    },
    // #3010
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright),
            Tag(Of),
            Tag(Nn),
            Tag(Uni),
            Label(AllRightReserved),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright),
            Tag(Of),
            Tag(Nn),
            Tag(Uni),
            AnyTag(&[Nn, Of]),
            Label(AllRightReserved),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright),
            Tag(Of),
            Tag(Nn),
            Tag(Uni),
            Tag(Nnp),
            Label(AllRightReserved),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright),
            Tag(Of),
            Tag(Nn),
            Tag(Uni),
            AnyTag(&[Nn, Of]),
            Tag(Nnp),
            Label(AllRightReserved),
        ],
    },
    // #3020
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nnp), Tag(Nn), Tag(Of), Label(Company)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Tag(Nnp),
            Tag(Nn),
            Tag(Of),
            Label(Company),
        ],
    },
    // #3030
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Company), Label(AllRightReserved), Label(Copyright2)],
    },
    // #3035
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright),
            Tag(Nn),
            Tag(Nn),
            AnyTag(&[Nn, Nnp]),
            Tag(By),
            Tag(Nn),
            Label(Name),
            Label(AllRightReserved),
        ],
    },
    // #3040
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
        ],
    },
    // #3050
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Tag(By), Label(Company)],
    },
    // #3060
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Label(NameCaps), Label(AndCo)],
    },
    // #3065
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright2),
            Tag(Nn),
            Label(NameCaps),
            Tag(Nn),
            Label(Name),
        ],
    },
    // #1567
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(NameYear)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Label(NameYear)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(NameYear), Tag(Auths)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Label(NameYear), Tag(Auths)],
    },
    // #15675
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Tag(Nnp), Label(YrRange)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Tag(Nn), Tag(Nnp), Label(YrRange)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Tag(Nn), Tag(Nnp), Label(YrRange)],
    },
    // #15676
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Tag(Cc), Label(YrRange)],
    },
    // #2841
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Label(AndCo)],
    },
    // #35011
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Tag(Nn), Tag(AuthDot)],
    },
    // #15800
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Company),
            Tag(Copy),
            Tag(Copy),
            Label(AllRightReserved),
            Tag(By),
            Label(Name),
        ],
    },
    // #157201
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Company),
            Tag(Copy),
            Tag(Copy),
            Label(AllRightReserved),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Company),
            Tag(Copy),
            Tag(Copy),
            Label(AllRightReserved),
            Label(YrRange),
        ],
    },
    // #15730
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(NameCopy),
            AnyTagOrLabel(&[Copy], &[NameCaps]),
            Label(AllRightReserved),
        ],
    },
    // #15674
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(AllRightReserved),
            AnyLabel(&[Name, Company]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(AllRightReserved),
            Tag(By),
            AnyLabel(&[Name, Company]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(AllRightReserved),
            AnyLabel(&[Name, Company]),
            Label(YrRange),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(AllRightReserved),
            Tag(By),
            Label(Name),
            AnyLabel(&[Name, Company]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(AllRightReserved),
            AnyLabel(&[Name, Company]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(AllRightReserved),
            Tag(By),
            AnyLabel(&[Name, Company]),
        ],
    },
    // #15680
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(AllRightReserved),
            AnyLabel(&[Name, NameYear, Company]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(AllRightReserved),
            AnyLabel(&[Name, NameYear, Company]),
        ],
    },
    // #15690
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(AllRightReserved),
            Label(DashCaps),
            Tag(Nnp),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Dash),
            Label(AllRightReserved),
            Label(DashCaps),
            Tag(Nnp),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(AllRightReserved),
            Label(DashCaps),
            Tag(Nnp),
        ],
    },
    // #15700
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(AllRightReserved),
            Tag(By),
            AnyLabel(&[Name, Company]),
            Tag(Nn),
            Label(Name),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(AllRightReserved),
            Tag(By),
            AnyLabel(&[Name, Company]),
            Tag(Nn),
            Label(Name),
        ],
    },
    // #15710
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(AllRightReserved),
            Tag(By),
            AnyLabel(&[Name, NameYear, Company]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(AllRightReserved),
            Tag(By),
            AnyLabel(&[Name, NameYear, Company]),
        ],
    },
    // #157111
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Label(AllRightReserved),
            Tag(Nnp),
            Label(Company),
            Label(YrRange),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Copy),
            Label(AllRightReserved),
            Tag(Nnp),
            Label(Company),
            Label(YrRange),
        ],
    },
    // #15720
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nnp), Label(NameYear)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nnp), Label(NameYear), Label(Company)],
    },
    // #rare-cd-not-year
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), AnyTag(&[Cd, Cds]), Label(Company)],
    },
    // #999991
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright2), Tag(Nn), Tag(Nn), Tag(Email)],
    },
    // #999992
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Company),
            Label(YrRange),
            Tag(Copy),
            Label(AllRightReserved),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Company),
            Label(YrRange),
            Tag(Copy),
            Tag(Copy),
            Label(AllRightReserved),
        ],
    },
    // #copydash-co
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Tag(Dash), Label(Company)],
    },
    // #83000
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Holder), Label(Name)],
    },
    // #83001
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Holder), Tag(Is), Label(NameEmail)],
    },
    // #83002
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Tag(Contributors)],
    },
    // #83002.1
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(By),
            AnyTag(&[Nn, Nnp]),
            AnyTag(&[Nn, Nnp]),
            AnyTag(&[Nn, Nnp]),
            Label(Name),
        ],
    },
    // #83002.2
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(By), Tag(Nn), Tag(Nn), Tag(AuthDot)],
    },
    // #83002.3 - "(C) by the respective authors," (Nn Nn Auths Cc)
    // Matches when "authors" is followed by a comma (like Python's AUTHDOT for "authors,")
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(By),
            Tag(Nn),
            Tag(Nn),
            AnyTag(&[Auth, Auths]),
            Tag(Cc),
        ],
    },
    // #83003
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(By), Tag(Nn), Tag(Nn)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(By), Tag(Nn), Tag(Nn), Tag(Maint)],
    },
    // #83004
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nnp), Tag(Auths)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nnp), Tag(Nnp), Tag(Auths)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Tag(Nnp), Tag(Auths)],
    },
    // #83020
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Tag(Maint)],
    },
    // #83030
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Nn), Tag(AuthDot)],
    },
    // #1615
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright),
            Tag(Nn),
            Label(YrRange),
            Tag(By),
            Label(Company),
        ],
    },
    // #157998
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Portions), Tag(Copy), Tag(Nn), Label(Name)],
    },
    // #2609.1
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Portions),
            Tag(Auth2),
            Label(InitialDev),
            Tag(Is),
            Tag(Copy),
            Label(InitialDev),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Portions),
            Tag(Auth2),
            Label(InitialDev),
            Tag(Is),
            Label(Copyright2),
            Label(InitialDev),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Portions),
            Tag(Auth2),
            Label(InitialDev),
            Tag(Is),
            Tag(Copy),
            Label(YrRange),
            Label(InitialDev),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Portions),
            Tag(Auth2),
            Label(InitialDev),
            Tag(Is),
            Label(Copyright2),
            Label(YrRange),
            Label(InitialDev),
        ],
    },
    // #2609.2
    GrammarRule {
        label: Copyright,
        pattern: &[AnyLabel(&[Copyright, Copyright2]), Label(InitialDev)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            AnyLabel(&[Copyright, Copyright2]),
            Label(InitialDev),
            Label(AllRightReserved),
        ],
    },
    // #35012
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(InitialDev)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), Label(InitialDev)],
    },
    // #157999.12
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Company), Tag(Copy), Label(NameYear)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Company), Tag(Copy), Tag(Copy), Label(NameYear)],
    },
    // #157999.13
    GrammarRule {
        label: NameEmail,
        pattern: &[Tag(Nnp), Label(NameEmail)],
    },
    // #157999.14
    GrammarRule {
        label: NameEmail,
        pattern: &[Tag(Dash), Label(NameEmail)],
    },
    GrammarRule {
        label: NameEmail,
        pattern: &[Tag(Dash), Label(NameEmail), Tag(Nn)],
    },
    // #157999.14 (second)
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright2),
            Tag(Following),
            Tag(Auths),
            Label(NameEmail),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright2),
            Tag(Following),
            Tag(Auths),
            Label(NameEmail),
            Label(NameEmail),
        ],
    },
    // #10989898
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Is),
            Tag(Held),
            Tag(By),
            AnyTagOrLabel(&[Nnp], &[Name, Company, NameEmail]),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Tag(Copy),
            Tag(Is),
            Tag(Held),
            Tag(By),
            AnyTagOrLabel(&[Nnp], &[Name, Company, NameEmail]),
            AnyTagOrLabel(&[Nnp], &[Name, Company, NameEmail]),
        ],
    },
    // =========================================================================
    // AUTHOR RULES (Python lines 3418–3483)
    // =========================================================================
    // #26382
    GrammarRule {
        label: Author,
        pattern: &[AnyTag(&[By, Maint]), Label(NameEmail)],
    },
    GrammarRule {
        label: Author,
        pattern: &[AnyTag(&[By, Maint]), Label(NameEmail), Label(YrRange)],
    },
    // #264000
    GrammarRule {
        label: Author,
        pattern: &[
            Tag(SpdxContrib),
            AnyTagOrLabel(&[Email], &[Company, Name, NameEmail, NameYear]),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            Tag(SpdxContrib),
            AnyTagOrLabel(&[Email], &[Company, Name, NameEmail, NameYear]),
            AnyTagOrLabel(&[Email, Nn], &[Company, Name, NameEmail, NameYear]),
        ],
    },
    // #2645-1
    GrammarRule {
        label: Author,
        pattern: &[Tag(Auth2), Tag(By), Label(Company), Tag(Nnp)],
    },
    GrammarRule {
        label: Author,
        pattern: &[Tag(Auth2), Tag(Auth2), Tag(By), Label(Company), Tag(Nnp)],
    },
    GrammarRule {
        label: Author,
        pattern: &[Tag(Auth2), Tag(By), AnyLabel(&[Name, NameEmail, NameYear])],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            Tag(Auth2),
            Tag(Auth2),
            Tag(By),
            AnyLabel(&[Name, NameEmail, NameYear]),
        ],
    },
    // #2650 expanded
    GrammarRule {
        label: Author,
        pattern: &[AnyTag(&[Auth, Contributors, Auths]), Tag(Email)],
    },
    GrammarRule {
        label: Author,
        pattern: &[AnyTag(&[Auth, Contributors, Auths]), Tag(Nn), Tag(Email)],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            AnyLabel(&[Company, Name]),
            Tag(Email),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            Tag(Nn),
            AnyLabel(&[Company, Name]),
            Tag(Email),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            Label(YrRange),
            Tag(Email),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[AnyTag(&[Auth, Contributors, Auths]), Tag(By), Tag(Email)],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            AnyLabel(&[Company, Name]),
            Tag(By),
            Tag(Email),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            Tag(Email),
            Label(Name),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            AnyTag(&[Auth, Contributors, Auths]),
            Tag(Email),
        ],
    },
    // #2660 (two-entity)
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            Tag(Nn),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
            Tag(Nn),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
            Label(YrRange),
        ],
    },
    // #2660 (single-entity)
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            Tag(Nn),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
            Label(YrRange),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            Tag(Nn),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
            Label(YrRange),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            AnyTag(&[Auth, Contributors, Auths]),
            AnyLabel(&[Company, Name, NameEmail, NameYear]),
        ],
    },
    // #2661
    GrammarRule {
        label: Author,
        pattern: &[
            Label(Author),
            Tag(Nn),
            Tag(Nn),
            Label(Name),
            Tag(Nn),
            Tag(Of),
            Tag(Nn),
            Label(Name),
        ],
    },
    // #2670
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            Label(YrRange),
            AnyLabel(&[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            Label(YrRange),
            Tag(By),
            AnyLabel(&[Company, Name, NameEmail]),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            Label(YrRange),
            AnyLabel(&[Company, Name, NameEmail]),
            AnyLabel(&[Company, Name, NameEmail]),
        ],
    },
    // #2680
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            AnyTagOrLabel(&[Nnp], &[YrRange]),
            AnyTagOrLabel(&[Nnp], &[YrRange]),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            AnyTagOrLabel(&[Nnp], &[YrRange]),
            AnyTagOrLabel(&[Nnp], &[YrRange]),
            AnyTagOrLabel(&[Nnp], &[YrRange]),
        ],
    },
    // #2690
    GrammarRule {
        label: Author,
        pattern: &[AnyTag(&[Auth, Contributors, Auths]), Label(YrRange)],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            AnyTag(&[Nn, Caps]),
            Label(YrRange),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            Label(YrRange),
            Label(YrRange),
        ],
    },
    // #2700
    GrammarRule {
        label: Author,
        pattern: &[
            AnyLabel(&[Company, Name, NameEmail]),
            AnyTag(&[Auth, Contributors, Auths]),
            Label(YrRange),
        ],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyLabel(&[Company, Name, NameEmail]),
            AnyLabel(&[Company, Name, NameEmail]),
            AnyTag(&[Auth, Contributors, Auths]),
            Label(YrRange),
        ],
    },
    // #2720
    GrammarRule {
        label: Author,
        pattern: &[Tag(By), Label(NameEmail)],
    },
    GrammarRule {
        label: Author,
        pattern: &[Tag(By), Tag(Cc), Label(NameEmail)],
    },
    GrammarRule {
        label: Author,
        pattern: &[Tag(By), Label(NameEmail), Label(NameEmail)],
    },
    // #2720 (second)
    GrammarRule {
        label: Author,
        pattern: &[AnyTag(&[Auth, Contributors, Auths]), Label(NameEmail)],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Contributors, Auths]),
            Label(NameEmail),
            Label(NameEmail),
        ],
    },
    // #2730
    GrammarRule {
        label: Author,
        pattern: &[Label(Author), Tag(Cc), AnyTag(&[Auth, Auths])],
    },
    GrammarRule {
        label: Author,
        pattern: &[Label(Author), Tag(Cc), Tag(Nn), AnyTag(&[Auth, Auths])],
    },
    // #2740
    GrammarRule {
        label: Author,
        pattern: &[Tag(By), Tag(Email)],
    },
    // #2750 ANDAUTH
    GrammarRule {
        label: AndAuth,
        pattern: &[Tag(Cc), AnyTagOrLabel(&[Auth, Contributors], &[Name])],
    },
    GrammarRule {
        label: AndAuth,
        pattern: &[
            Tag(Cc),
            AnyTagOrLabel(&[Auth, Contributors], &[Name]),
            AnyTagOrLabel(&[Auth, Contributors], &[Name]),
        ],
    },
    // #2760
    GrammarRule {
        label: Author,
        pattern: &[Label(Author), Label(AndAuth)],
    },
    GrammarRule {
        label: Author,
        pattern: &[Label(Author), Label(AndAuth), Label(AndAuth)],
    },
    // #2761
    GrammarRule {
        label: Author,
        pattern: &[AnyTag(&[Auth, Auths, Auth2]), Tag(Nnp), Tag(Cc), Tag(Pn)],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            AnyTag(&[Auth, Auths, Auth2]),
            Tag(By),
            Tag(Nnp),
            Tag(Cc),
            Tag(Pn),
        ],
    },
    // #2762
    GrammarRule {
        label: Author,
        pattern: &[Label(Author), Tag(Nn), AnyLabel(&[Name, Company])],
    },
    GrammarRule {
        label: Author,
        pattern: &[
            Label(Author),
            Tag(Nn),
            AnyLabel(&[Name, Company]),
            AnyLabel(&[Name, Company]),
        ],
    },
    // #2645-4
    GrammarRule {
        label: Author,
        pattern: &[
            Tag(Auth2),
            Tag(Cc),
            Label(Author),
            Tag(Nn),
            Label(Name),
            Tag(Nn),
            Tag(Nn),
            Tag(Nnp),
        ],
    },
    // #2645-7
    GrammarRule {
        label: Author,
        pattern: &[Tag(Auth2), Label(Company)],
    },
    GrammarRule {
        label: Author,
        pattern: &[Tag(Auth2), Label(Company), Label(Name)],
    },
    // #not-attributable
    GrammarRule {
        label: Author,
        pattern: &[Tag(Auth), Tag(Nn), Tag(Nnp)],
    },
    // #author-Foo-Bar
    GrammarRule {
        label: Author,
        pattern: &[Tag(Auth), Tag(Nnp), Tag(Nnp)],
    },
    GrammarRule {
        label: Author,
        pattern: &[Tag(Auth), Tag(Nnp), Tag(Nnp), Tag(Nnp)],
    },
    // #Author-Foo-joe@email.com
    GrammarRule {
        label: Author,
        pattern: &[Tag(Auth), Tag(Nnp), Tag(Email)],
    },
    GrammarRule {
        label: Author,
        pattern: &[Tag(Auth), Tag(Nnp), Tag(Nnp), Tag(Email)],
    },
    GrammarRule {
        label: Author,
        pattern: &[Tag(Auth), Tag(Nnp), Tag(Email), Tag(Email)],
    },
    // #Atkinson-et-al
    GrammarRule {
        label: Author,
        pattern: &[Tag(Auth), Tag(Nnp), Tag(Cc), Tag(AuthDot)],
    },
    GrammarRule {
        label: Author,
        pattern: &[Tag(Auth), Tag(Nnp), Tag(Nnp), Tag(Cc), Tag(AuthDot)],
    },
    // =========================================================================
    // MIXED AUTHOR AND COPYRIGHT (Python lines 3485–3530)
    // =========================================================================
    // #2800-1
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(Author)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(By), Label(Author)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(Author), Label(YrRange)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(By), Label(Author), Label(YrRange)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Label(Author), Label(Author)],
    },
    // #2820
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Author), Label(Copyright2)],
    },
    // #2840 (mixed)
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(By), Tag(Mit)],
    },
    // #3000 (mixed)
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Copyright2),
            Tag(Nn),
            Label(NameCaps),
            Tag(Nn),
            Tag(Nn),
            Tag(Auths),
        ],
    },
    // #4200
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Author),
            Tag(Nn),
            Label(YrRange),
            Label(Copyright2),
            Label(AllRightReserved),
        ],
    },
    // #420121
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Copyright), Tag(Contributors)],
    },
    // =========================================================================
    // LAST RESORT CATCH-ALL (Python lines 3515–3530)
    // =========================================================================
    // #99900
    GrammarRule {
        label: Copyright,
        pattern: &[Label(Company), Tag(Copy), Label(AllRightReserved)],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            Label(Company),
            Tag(Copy),
            Tag(Copy),
            Label(AllRightReserved),
        ],
    },
    // #99999 catch-all (broad matcher, expanded to 1-3 middle elements)
    GrammarRule {
        label: Copyright,
        pattern: &[
            AnyTagOrLabel(&[Copy], &[Copyright, Copyright2, NameCopy]),
            AnyTagOrLabel(
                &[
                    Copy, Nnp, AuthDot, Caps, Cd, Cds, Pn, Comp, Uni, Cc, Of, In, By, Oth, Van,
                    Email, MixedCap, Nn,
                ],
                &[
                    YrRange, Name, NameEmail, NameYear, NameCopy, NameCaps, Company,
                ],
            ),
            Label(AllRightReserved),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            AnyTagOrLabel(&[Copy], &[Copyright, Copyright2, NameCopy]),
            AnyTagOrLabel(
                &[
                    Copy, Nnp, AuthDot, Caps, Cd, Cds, Pn, Comp, Uni, Cc, Of, In, By, Oth, Van,
                    Email, MixedCap, Nn,
                ],
                &[
                    YrRange, Name, NameEmail, NameYear, NameCopy, NameCaps, Company,
                ],
            ),
            AnyTagOrLabel(
                &[
                    Copy, Nnp, AuthDot, Caps, Cd, Cds, Pn, Comp, Uni, Cc, Of, In, By, Oth, Van,
                    Email, MixedCap, Nn,
                ],
                &[
                    YrRange, Name, NameEmail, NameYear, NameCopy, NameCaps, Company,
                ],
            ),
            Label(AllRightReserved),
        ],
    },
    GrammarRule {
        label: Copyright,
        pattern: &[
            AnyTagOrLabel(&[Copy], &[Copyright, Copyright2, NameCopy]),
            AnyTagOrLabel(
                &[
                    Copy, Nnp, AuthDot, Caps, Cd, Cds, Pn, Comp, Uni, Cc, Of, In, By, Oth, Van,
                    Email, MixedCap, Nn,
                ],
                &[
                    YrRange, Name, NameEmail, NameYear, NameCopy, NameCaps, Company,
                ],
            ),
            AnyTagOrLabel(
                &[
                    Copy, Nnp, AuthDot, Caps, Cd, Cds, Pn, Comp, Uni, Cc, Of, In, By, Oth, Van,
                    Email, MixedCap, Nn,
                ],
                &[
                    YrRange, Name, NameEmail, NameYear, NameCopy, NameCaps, Company,
                ],
            ),
            AnyTagOrLabel(
                &[
                    Copy, Nnp, AuthDot, Caps, Cd, Cds, Pn, Comp, Uni, Cc, Of, In, By, Oth, Van,
                    Email, MixedCap, Nn,
                ],
                &[
                    YrRange, Name, NameEmail, NameYear, NameCopy, NameCaps, Company,
                ],
            ),
            Label(AllRightReserved),
        ],
    },
    // #9999970
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), Tag(Copy), AnyTag(&[Cd, Cds]), Label(NameEmail)],
    },
    // #9999981 — merge orphaned Copy before a Copyright tree
    GrammarRule {
        label: Copyright,
        pattern: &[Tag(Copy), AnyLabel(&[Copyright, Copyright2])],
    },
    // #999990
    GrammarRule {
        label: Copyright,
        pattern: &[
            AnyTagOrLabel(&[Copy], &[NameCopy]),
            AnyTagOrLabel(&[Copy], &[NameCopy]),
        ],
    },
    // #99900111
    GrammarRule {
        label: Copyright,
        pattern: &[AnyLabel(&[Copyright, Copyright2]), Label(AllRightReserved)],
    },
];
