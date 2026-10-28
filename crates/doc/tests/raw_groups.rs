use cadcraft_doc::{Handle, foreign::dimassoc_reference};

#[test]
fn dimassoc_reference_ignores_application_groups_and_normalizes_the_subclass() {
    let groups = [(102, "{OTHER"), (100, "AcDbDimAssoc"), (330, "99"), (102, "}"), (100, " AcDbDimAssoc "), (330, "002a")];
    assert_eq!(dimassoc_reference(groups), Some((4, Handle(0x2A))), "a class-looking application group must not supply the dimension owner");
}
