use super::*;
use crate::copyright::types::TreeLabel;

#[test]
fn test_grammar_rules_not_empty() {
    assert!(!GRAMMAR_RULES.is_empty());
}

#[test]
fn test_rule_count_is_substantial() {
    assert!(
        GRAMMAR_RULES.len() >= 400,
        "Expected at least 400 rules, got {}",
        GRAMMAR_RULES.len()
    );
}

#[test]
fn test_year_rules_exist() {
    let yr_rules: Vec<_> = GRAMMAR_RULES
        .iter()
        .filter(|r| r.label == TreeLabel::YrRange)
        .collect();
    assert!(!yr_rules.is_empty(), "Should have YR-RANGE rules");
    assert!(
        yr_rules.len() >= 10,
        "Expected at least 10 YR-RANGE rules, got {}",
        yr_rules.len()
    );
}

#[test]
fn test_yr_and_rules_exist() {
    let yr_and_rules: Vec<_> = GRAMMAR_RULES
        .iter()
        .filter(|r| r.label == TreeLabel::YrAnd)
        .collect();
    assert!(!yr_and_rules.is_empty(), "Should have YR-AND rules");
}

#[test]
fn test_all_right_reserved_rules_exist() {
    let arr_rules: Vec<_> = GRAMMAR_RULES
        .iter()
        .filter(|r| r.label == TreeLabel::AllRightReserved)
        .collect();
    assert!(!arr_rules.is_empty(), "Should have ALLRIGHTRESERVED rules");
    assert_eq!(
        arr_rules.len(),
        2,
        "Should have 2 ALLRIGHTRESERVED rules (with/without optional)"
    );
}

#[test]
fn test_name_rules_exist() {
    let name_rules: Vec<_> = GRAMMAR_RULES
        .iter()
        .filter(|r| r.label == TreeLabel::Name)
        .collect();
    assert!(!name_rules.is_empty(), "Should have NAME rules");
    assert!(
        name_rules.len() >= 30,
        "Expected at least 30 NAME rules, got {}",
        name_rules.len()
    );
}

#[test]
fn test_company_rules_exist() {
    let company_rules: Vec<_> = GRAMMAR_RULES
        .iter()
        .filter(|r| r.label == TreeLabel::Company)
        .collect();
    assert!(!company_rules.is_empty(), "Should have COMPANY rules");
    assert!(
        company_rules.len() >= 40,
        "Expected at least 40 COMPANY rules, got {}",
        company_rules.len()
    );
}

#[test]
fn test_andco_rules_exist() {
    let andco_rules: Vec<_> = GRAMMAR_RULES
        .iter()
        .filter(|r| r.label == TreeLabel::AndCo)
        .collect();
    assert!(!andco_rules.is_empty(), "Should have ANDCO rules");
}

#[test]
fn test_name_email_rules_exist() {
    let ne_rules: Vec<_> = GRAMMAR_RULES
        .iter()
        .filter(|r| r.label == TreeLabel::NameEmail)
        .collect();
    assert!(!ne_rules.is_empty(), "Should have NAME-EMAIL rules");
    assert!(
        ne_rules.len() >= 4,
        "Expected at least 4 NAME-EMAIL rules, got {}",
        ne_rules.len()
    );
}

#[test]
fn test_name_year_rules_exist() {
    let ny_rules: Vec<_> = GRAMMAR_RULES
        .iter()
        .filter(|r| r.label == TreeLabel::NameYear)
        .collect();
    assert!(!ny_rules.is_empty(), "Should have NAME-YEAR rules");
    assert!(
        ny_rules.len() >= 10,
        "Expected at least 10 NAME-YEAR rules, got {}",
        ny_rules.len()
    );
}

#[test]
fn test_dashcaps_rules_exist() {
    let dc_rules: Vec<_> = GRAMMAR_RULES
        .iter()
        .filter(|r| r.label == TreeLabel::DashCaps)
        .collect();
    assert_eq!(dc_rules.len(), 1, "Should have exactly 1 DASHCAPS rule");
}

#[test]
fn test_initialdev_rules_exist() {
    let id_rules: Vec<_> = GRAMMAR_RULES
        .iter()
        .filter(|r| r.label == TreeLabel::InitialDev)
        .collect();
    assert_eq!(id_rules.len(), 2, "Should have 2 INITIALDEV rules");
}

#[test]
fn test_copyright_rules_exist() {
    let cr_rules: Vec<_> = GRAMMAR_RULES
        .iter()
        .filter(|r| r.label == TreeLabel::Copyright)
        .collect();
    assert!(
        cr_rules.len() >= 100,
        "Expected at least 100 COPYRIGHT rules, got {}",
        cr_rules.len()
    );
}

#[test]
fn test_copyright2_rules_exist() {
    let cr2_rules: Vec<_> = GRAMMAR_RULES
        .iter()
        .filter(|r| r.label == TreeLabel::Copyright2)
        .collect();
    assert!(
        cr2_rules.len() >= 30,
        "Expected at least 30 COPYRIGHT2 rules, got {}",
        cr2_rules.len()
    );
}

#[test]
fn test_author_rules_exist() {
    let auth_rules: Vec<_> = GRAMMAR_RULES
        .iter()
        .filter(|r| r.label == TreeLabel::Author)
        .collect();
    assert!(
        auth_rules.len() >= 30,
        "Expected at least 30 AUTHOR rules, got {}",
        auth_rules.len()
    );
}

#[test]
fn test_andauth_rules_exist() {
    let andauth_rules: Vec<_> = GRAMMAR_RULES
        .iter()
        .filter(|r| r.label == TreeLabel::AndAuth)
        .collect();
    assert!(
        andauth_rules.len() >= 2,
        "Expected at least 2 ANDAUTH rules, got {}",
        andauth_rules.len()
    );
}

#[test]
fn test_name_copy_rules_exist() {
    let nc_rules: Vec<_> = GRAMMAR_RULES
        .iter()
        .filter(|r| r.label == TreeLabel::NameCopy)
        .collect();
    assert_eq!(nc_rules.len(), 1, "Should have 1 NAME-COPY rule");
}

#[test]
fn test_name_caps_rules_exist() {
    let nc_rules: Vec<_> = GRAMMAR_RULES
        .iter()
        .filter(|r| r.label == TreeLabel::NameCaps)
        .collect();
    assert!(
        nc_rules.len() >= 3,
        "Expected at least 3 NAME-CAPS rules, got {}",
        nc_rules.len()
    );
}

#[test]
fn test_all_patterns_non_empty() {
    for (i, rule) in GRAMMAR_RULES.iter().enumerate() {
        assert!(
            !rule.pattern.is_empty(),
            "Rule {} ({:?}) has empty pattern",
            i,
            rule.label
        );
    }
}

#[test]
fn test_basic_yr_range_pattern() {
    let single_yr = GRAMMAR_RULES
        .iter()
        .find(|r| r.label == TreeLabel::YrRange && r.pattern.len() == 1);
    assert!(
        single_yr.is_some(),
        "Should have a single-element YR-RANGE rule for bare YR"
    );
}
