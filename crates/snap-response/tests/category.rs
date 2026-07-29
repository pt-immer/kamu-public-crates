use kamu_snap_response::Category;

#[test]
fn labels_display_and_serde_are_stable() {
    let cases = [
        (Category::Success, "Success"),
        (Category::Message, "Message"),
        (Category::Business, "Business"),
        (Category::System, "System"),
    ];

    for (category, label) in cases {
        assert_eq!(category.as_str(), label);
        assert_eq!(category.to_string(), label);
        let wire = serde_json::to_string(&category).unwrap();
        assert_eq!(serde_json::from_str::<Category>(&wire).unwrap(), category);
    }
}
