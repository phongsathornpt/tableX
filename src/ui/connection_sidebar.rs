use crate::ui::DatabaseWorkspace;
use gpui_kit::base::h_flex;
use gpui_kit::component::{
    IconName,
    sidebar::{Sidebar, SidebarFooter, SidebarGroup, SidebarHeader, SidebarMenu, SidebarMenuItem},
};
use gpui_kit::{Context, IntoElement, ParentElement as _, Styled as _};

pub fn render(selected_connection: &str, cx: &mut Context<DatabaseWorkspace>) -> impl IntoElement {
    Sidebar::new("connections")
        .w_64()
        .header(
            SidebarHeader::new().child(
                h_flex()
                    .gap_2()
                    .items_center()
                    .child(IconName::Folder)
                    .child("tableX"),
            ),
        )
        .child(
            SidebarGroup::new("Connections").child(
                SidebarMenu::new().child(
                    SidebarMenuItem::new(selected_connection.to_string())
                        .active(true)
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.select_default_connection(cx);
                        })),
                ),
            ),
        )
        .child(
            SidebarGroup::new("Actions").child(
                SidebarMenu::new()
                    .child(
                        SidebarMenuItem::new("New connection")
                            .on_click(|_, _, _| println!("new connection requested")),
                    )
                    .child(
                        SidebarMenuItem::new("Refresh metadata")
                            .on_click(|_, _, _| println!("metadata refresh requested")),
                    ),
            ),
        )
        .footer(SidebarFooter::new().child("PostgreSQL first"))
}
