//! The app menu item validator (`App::on_validate_menu_item`), asked through the
//! test platform as the macOS platform asks it each time an item's menu opens.

use gpui::{Action as _, MenuItemState, TestAppContext, actions};

actions!(menu_validation_test, [Bound, Unbound]);

#[gpui::test]
fn the_default_menu_item_state_is_whether_the_action_is_available(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.on_action(|_: &Bound, _| {});
    });

    let bound = cx.validate_menu_item(&Bound).expect("a recorded validator");
    let unbound = cx.validate_menu_item(&Unbound).expect("a recorded validator");

    assert_eq!(bound, MenuItemState { enabled: true, checked: None, title: None });
    assert!(!unbound.enabled);
    assert_eq!((unbound.checked, unbound.title), (None, None));
}

#[gpui::test]
fn an_installed_validator_answers_for_the_platform(cx: &mut TestAppContext) {
    cx.update(|cx| {
        cx.on_validate_menu_item(|action, _| MenuItemState {
            enabled: action.as_any().is::<Unbound>(),
            checked: Some(true),
            title: Some(action.name().into()),
        })
    });

    let state = cx.validate_menu_item(&Unbound).expect("a recorded validator");

    assert_eq!(state, MenuItemState { enabled: true, checked: Some(true), title: Some(Unbound.name().into()) });
    assert!(!cx.validate_menu_item(&Bound).unwrap().enabled);
}
