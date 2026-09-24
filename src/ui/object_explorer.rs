use crate::domain::database_object::DatabaseObject;
use crate::ui::DatabaseWorkspace;
use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::{
    ActiveTheme as _, IconName, Sizable as _,
    button::{Button, ButtonVariants as _},
    list::ListItem,
    scroll::ScrollableElement as _,
    tree::{TreeItem, TreeState, tree},
};
use gpui_kit::{
    AppContext as _, Context, Entity, FontWeight, IntoElement, ParentElement as _, Styled as _,
    div, px,
};

pub struct ObjectExplorer {
    tree_state: Entity<TreeState>,
}

impl ObjectExplorer {
    pub fn new(objects: Vec<DatabaseObject>, cx: &mut Context<DatabaseWorkspace>) -> Self {
        let tree_state = cx.new(|cx| {
            TreeState::new(cx).items(objects.into_iter().map(to_tree_item).collect::<Vec<_>>())
        });
        Self { tree_state }
    }

    pub fn render(&self, cx: &mut Context<DatabaseWorkspace>) -> impl IntoElement {
        v_flex()
            .w_64()
            .h_full()
            .border_r_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().background)
            .child(
                h_flex()
                    .justify_between()
                    .items_center()
                    .px_3()
                    .py_2()
                    .child(
                        div()
                            .text_sm()
                            .font_weight(FontWeight::SEMIBOLD)
                            .child("Database explorer"),
                    )
                    .child(
                        Button::new("refresh-tree")
                            .ghost()
                            .xsmall()
                            .tooltip("Refresh database objects")
                            .on_click(|_, _, _| println!("refresh tree requested")),
                    ),
            )
            .child(div().flex_1().overflow_y_scrollbar().child(tree(
                &self.tree_state,
                |ix, entry, selected, _, _| {
                    ListItem::new(ix)
                        .selected(selected)
                        .pl(px(12.) + px(16.) * entry.depth())
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(IconName::Folder)
                                .child(entry.item().label.clone()),
                        )
                },
            )))
    }
}

fn to_tree_item(object: DatabaseObject) -> TreeItem {
    let mut item = TreeItem::new(object.id, object.label).expanded(!object.children.is_empty());
    for child in object.children {
        item = item.child(to_tree_item(child));
    }
    item
}
