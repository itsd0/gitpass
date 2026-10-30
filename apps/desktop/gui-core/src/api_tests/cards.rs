use super::*;
use crate::api::display::is_secret_copy;


#[test]
fn card_brand_and_labels() {
    for (n, b) in [("4111111111111111", Some("Visa")), ("5555555555554444", Some("Mastercard")), ("2223003122003222", Some("Mastercard")),
                   ("378282246310005", Some("Amex")), ("6011111111111117", Some("Discover")), ("3530111333300000", Some("JCB")),
                   ("30569309025904", Some("Diners Club")), ("6200000000000005", Some("UnionPay")), ("9999000011117575", None)] {
        assert_eq!(card_brand(n), b, "{n}");
    }
    assert_eq!(card_label("4000123412347575"), "Visa •••• 7575");
    assert_eq!(card_label("9999000011117575"), "•••• 7575");
    assert_eq!(card_mask("4000123412347575"), "•••• •••• •••• 7575");
    assert!(!card_label("4000123412347575").contains("4000"));
}

#[test]
fn card_subtitle_with_cardholder_first_and_imported_labels() {
    let e = env();
    let mk = |fields: Vec<Field>| e.app.create_item(&ItemInput { vault_id: e.vault.clone(), category: Category::CreditCard, title: "C".into(), url: None,
        sections: vec![Section { id: "main".into(), title: None, fields }], notes: None, tags: None, favorite: None, related_item_ids: None }).unwrap();
    // template order: cardholder name comes before the number
    let a = mk(vec![Field { id: "cardholder".into(), label: "cardholder name".into(), kind: FieldKind::Text, value: Some("Demo User".into()), masked: None, diff: None },
                    Field { id: "type".into(), label: "type".into(), kind: FieldKind::Text, value: Some("Visa".into()), masked: None, diff: None },
                    fld("number", FieldKind::Concealed, Some("4000 1234 5678 4432")), fld("cvv", FieldKind::Concealed, Some("999"))]);
    assert_eq!(a.subtitle, "Visa •••• 4432");
    // imported card: random field id, label "Card Number"
    let b = mk(vec![Field { id: "f_x1".into(), label: "Card Number".into(), kind: FieldKind::Concealed, value: Some("5555555555554444".into()), masked: None, diff: None }]);
    assert_eq!(b.subtitle, "Mastercard •••• 4444");
    let listed = e.app.list_items(&Filter { kind: FilterKind::All, vault_id: None, tag: None, category: None, query: None, sort: None }).unwrap();
    assert!(listed.iter().any(|s| s.subtitle == "Visa •••• 4432"));
}

#[test]
fn bank_account_and_server_crud_masking_and_subtitles() {
    let e = env();

    // 1. Bank Account creation and masking
    let bank_item = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::BankAccount,
        title: "Personal Checking".into(),
        url: None,
        sections: vec![Section {
            id: "main".into(),
            title: None,
            fields: vec![
                fld("bank_name", FieldKind::Text, Some("Chase")),
                fld("account_holder", FieldKind::Text, Some("Alice Doe")),
                fld("account_type", FieldKind::Text, Some("Checking")),
                fld("account_number", FieldKind::Concealed, Some("123456789012")),
                fld("routing_number", FieldKind::Text, Some("021000021")),
                fld("pin", FieldKind::Concealed, Some("4321")),
            ],
        }],
        notes: Some("Checking account".into()),
        tags: Some(vec!["finance".into()]),
        favorite: Some(true), related_item_ids: None }).unwrap();

    assert_eq!(bank_item.category, Category::BankAccount);
    assert_eq!(bank_item.subtitle, "Checking •••• 9012");

    let detail = e.app.get_item(&e.vault, &bank_item.id).unwrap();
    let fields = detail.sections[0].fields.clone();
    let acct_f = fields.iter().find(|f| f.id == "account_number").unwrap();
    assert_eq!(acct_f.kind, FieldKind::Concealed);
    assert_eq!(acct_f.value, None);
    assert_eq!(acct_f.masked.as_deref(), Some("•••• •••• •••• 9012"));

    let pin_f = fields.iter().find(|f| f.id == "pin").unwrap();
    assert_eq!(pin_f.kind, FieldKind::Concealed);
    assert_eq!(pin_f.value, None);
    assert!(pin_f.masked.is_none());

    // Reveal field
    let revealed_acct = e.app.reveal_field(&e.vault, &bank_item.id, "account_number").unwrap();
    assert_eq!(&*revealed_acct, "123456789012");

    // Copy field secrecy
    assert!(is_secret_copy(Category::BankAccount, &gitpass_core::item::Field {
        id: "account_number".into(),
        label: "account number".into(),
        kind: FieldKind::Concealed,
        value: Some("123456789012".into()),
        removed: false,
    }));
    assert!(is_secret_copy(Category::BankAccount, &gitpass_core::item::Field {
        id: "pin".into(),
        label: "PIN".into(),
        kind: FieldKind::Concealed,
        value: Some("4321".into()),
        removed: false,
    }));
    assert!(!is_secret_copy(Category::BankAccount, &gitpass_core::item::Field {
        id: "bank_name".into(),
        label: "bank name".into(),
        kind: FieldKind::Text,
        value: Some("Chase".into()),
        removed: false,
    }));

    // 2. Server creation and subtitle
    let srv_item = e.app.create_item(&ItemInput {
        vault_id: e.vault.clone(),
        category: Category::Server,
        title: "Web Node 1".into(),
        url: Some("10.0.0.50".into()),
        sections: vec![Section {
            id: "main".into(),
            title: None,
            fields: vec![
                fld("url", FieldKind::Url, Some("10.0.0.50")),
                fld("username", FieldKind::Text, Some("ubuntu")),
                fld("password", FieldKind::Concealed, Some("ServerPassword123!")),
                fld("admin_console_url", FieldKind::Url, Some("https://aws.amazon.com")),
                fld("hosting_provider", FieldKind::Text, Some("AWS")),
            ],
        }],
        notes: None,
        tags: None,
        favorite: None, related_item_ids: None }).unwrap();

    assert_eq!(srv_item.category, Category::Server);
    assert_eq!(srv_item.subtitle, "ubuntu@10.0.0.50");

    let srv_detail = e.app.get_item(&e.vault, &srv_item.id).unwrap();
    assert!(srv_detail.password_strength.is_some());

}

