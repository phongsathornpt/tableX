use gpui_kit::base::{h_flex, v_flex};
use gpui_kit::component::menu::DropdownMenu as _;
use gpui_kit::component::{
    ActiveTheme as _, Disableable as _, Sizable as _,
    button::{Button, ButtonVariants as _},
    input::{Input, InputContentType, InputState},
};
use gpui_kit::{
    AppContext as _, Context, IntoElement, ParentElement as _, Styled as _, Window, div,
};

use super::{DatabaseWorkspace, ssl_description, ssl_label, ssl_menu_item};
use crate::domain::connection::ConnectionId;
use crate::infrastructure::postgres::model::{PostgresConnectionProfile, PostgresSslMode};

pub(crate) struct ConnectionEditor {
    pub(crate) editing_id: Option<ConnectionId>,
    pub(crate) name: gpui_kit::Entity<InputState>,
    pub(crate) host: gpui_kit::Entity<InputState>,
    pub(crate) port: gpui_kit::Entity<InputState>,
    pub(crate) database: gpui_kit::Entity<InputState>,
    pub(crate) user: gpui_kit::Entity<InputState>,
    pub(crate) password: gpui_kit::Entity<InputState>,
    pub(crate) ca_certificate_path: gpui_kit::Entity<InputState>,
    pub(crate) ssl: PostgresSslMode,
    pub(crate) reject_unauthorized: bool,
}

impl ConnectionEditor {
    pub(crate) fn new(
        window: &mut Window,
        cx: &mut Context<DatabaseWorkspace>,
        profile: Option<&PostgresConnectionProfile>,
    ) -> Self {
        let value = |value: Option<&String>, fallback: &str| {
            value.map(String::as_str).unwrap_or(fallback).to_owned()
        };
        Self {
            editing_id: profile.map(|profile| profile.id.clone()),
            name: cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(value(profile.map(|p| &p.name), ""))
                    .placeholder("e.g. Production")
            }),
            host: cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(value(profile.map(|p| &p.host), "localhost"))
                    .placeholder("localhost or db.example.com")
            }),
            port: cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(profile.map_or_else(|| "5432".into(), |p| p.port.to_string()))
                    .placeholder("5432")
            }),
            database: cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(value(profile.map(|p| &p.database), "postgres"))
                    .placeholder("postgres")
            }),
            user: cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(value(profile.map(|p| &p.user), "postgres"))
                    .placeholder("postgres")
            }),
            password: cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(profile.and_then(|p| p.password.clone()).unwrap_or_default())
                    .placeholder("Optional for local auth")
            }),
            ca_certificate_path: cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(
                        profile
                            .and_then(|p| p.ca_certificate_path.clone())
                            .unwrap_or_default(),
                    )
                    .placeholder("/path/to/rds-ca.pem")
            }),
            ssl: profile.map(|profile| profile.ssl).unwrap_or_default(),
            reject_unauthorized: profile
                .map(|profile| profile.reject_unauthorized)
                .unwrap_or(true),
        }
    }

    pub(crate) fn profile(
        &self,
        cx: &Context<DatabaseWorkspace>,
        id: ConnectionId,
    ) -> Result<PostgresConnectionProfile, String> {
        let value = |state: &gpui_kit::Entity<InputState>| state.read(cx).value().to_string();
        let name = value(&self.name).trim().to_string();
        let host = value(&self.host).trim().to_string();
        let database = value(&self.database).trim().to_string();
        let user = value(&self.user).trim().to_string();
        let password = value(&self.password);
        let ca_certificate_path = value(&self.ca_certificate_path);
        let port = value(&self.port).trim().parse::<u16>().ok();

        let Some(port) = port.filter(|port| *port > 0) else {
            return Err("Port must be a number between 1 and 65535".into());
        };
        if name.is_empty() || host.is_empty() || database.is_empty() || user.is_empty() {
            return Err("Name, host, database, and user are required".into());
        }
        if self.ssl == PostgresSslMode::Require && !self.reject_unauthorized {
            return Err(
                "SSL mode 'require' must verify the server certificate; choose 'prefer' to use the insecure compatibility option".into(),
            );
        }

        Ok(PostgresConnectionProfile {
            id,
            name,
            host,
            port,
            database,
            user,
            password: (!password.is_empty()).then_some(password),
            ssl: self.ssl,
            reject_unauthorized: self.reject_unauthorized,
            ca_certificate_path: (!ca_certificate_path.trim().is_empty())
                .then_some(ca_certificate_path.trim().to_owned()),
        })
    }

    pub(crate) fn render(
        &self,
        test_running: bool,
        cx: &mut Context<DatabaseWorkspace>,
    ) -> impl IntoElement {
        v_flex()
            .w_full()
            .gap_4()
            .p_5()
            .rounded_md()
            .border_1()
            .border_color(cx.theme().border)
            .bg(cx.theme().secondary)
            .child(
                v_flex()
                    .gap_1()
                    .child(
                        div()
                            .text_lg()
                            .font_weight(gpui_kit::FontWeight::SEMIBOLD)
                            .child(if self.editing_id.is_some() {
                                "Edit PostgreSQL connection"
                            } else {
                                "Add PostgreSQL connection"
                            }),
                    )
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child(if self.editing_id.is_some() {
                                "Passwords are stored in the operating system credential store, never in the connection file. Leave blank to keep the saved password."
                            } else {
                                "Passwords are stored in the operating system credential store, never in the connection file."
                            }),
                    ),
            )
            .child(self.field("Name", &self.name, "e.g. Production"))
            .child(self.field("Host", &self.host, "localhost or db.example.com"))
            .child(
                h_flex()
                    .gap_3()
                    .child(self.field("Port", &self.port, "5432"))
                    .child(self.field("Database", &self.database, "postgres")),
            )
            .child(
                h_flex()
                    .gap_3()
                    .child(self.field("User", &self.user, "postgres"))
                    .child(self.field_with_type(
                        "Password",
                        &self.password,
                        "Optional for local auth",
                        Some(InputContentType::Password),
                    )),
            )
            .child(self.ssl_field(cx))
            .child(self.certificate_field(cx))
            .child(
                h_flex()
                    .justify_end()
                    .gap_2()
                    .child(
                        Button::new("test-connection")
                            .outline()
                            .small()
                            .label(if test_running { "Testing..." } else { "Test connection" })
                            .disabled(test_running)
                            .on_click(cx.listener(|this, _, _, cx| this.test_connection(cx))),
                    )
                    .child(
                        Button::new("cancel-connection")
                            .outline()
                            .small()
                            .label("Cancel")
                            .on_click(
                                cx.listener(|this, _, _, cx| this.close_connection_editor(cx)),
                            ),
                    )
                    .child(
                        Button::new("save-connection")
                            .primary()
                            .small()
                            .label(if self.editing_id.is_some() {
                                "Save changes"
                            } else {
                                "Save connection"
                            })
                            .disabled(test_running)
                            .on_click(cx.listener(|this, _, _, cx| this.save_connection(cx))),
                    ),
            )
    }

    fn field(
        &self,
        label: &'static str,
        state: &gpui_kit::Entity<InputState>,
        placeholder: &'static str,
    ) -> impl IntoElement {
        self.field_with_type(label, state, placeholder, None)
    }

    fn field_with_type(
        &self,
        label: &'static str,
        state: &gpui_kit::Entity<InputState>,
        _placeholder: &'static str,
        content_type: Option<InputContentType>,
    ) -> impl IntoElement {
        let mut input = Input::new(state).w_full();
        if let Some(content_type) = content_type {
            input = input.content_type(content_type).mask_toggle();
        }
        v_flex()
            .flex_1()
            .gap_1()
            .child(div().text_sm().child(label))
            .child(input)
    }

    fn ssl_field(&self, cx: &mut Context<DatabaseWorkspace>) -> impl IntoElement {
        let workspace = cx.entity();
        let selected = self.ssl;

        v_flex()
            .gap_1()
            .child(div().text_sm().child("SSL mode"))
            .child(
                Button::new("ssl-mode")
                    .outline()
                    .small()
                    .w_full()
                    .justify_between()
                    .label(ssl_label(selected))
                    .dropdown_caret(true)
                    .dropdown_menu(move |menu, _, _| {
                        menu.item(ssl_menu_item(
                            "Prefer",
                            "Use TLS when available; allow plaintext only if the server declines TLS",
                            PostgresSslMode::Prefer,
                            selected,
                            workspace.clone(),
                        ))
                        .item(ssl_menu_item(
                            "Require",
                            "Require a trusted TLS connection",
                            PostgresSslMode::Require,
                            selected,
                            workspace.clone(),
                        ))
                        .item(ssl_menu_item(
                            "Disable",
                            "Use plaintext; only for trusted local networks",
                            PostgresSslMode::Disable,
                            selected,
                            workspace.clone(),
                        ))
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(ssl_description(selected)),
            )
    }

    fn certificate_field(&self, cx: &mut Context<DatabaseWorkspace>) -> impl IntoElement {
        let workspace = cx.entity();
        let reject_unauthorized = self.reject_unauthorized;
        v_flex()
            .gap_2()
            .child(
                gpui_kit::component::checkbox::Checkbox::new("verify-server-certificate")
                    .label("Verify server certificate")
                    .checked(reject_unauthorized)
                    .on_change(move |checked, _, cx| {
                        workspace.update(cx, |workspace, cx| {
                            if let Some(editor) = &mut workspace.connection_editor {
                                editor.reject_unauthorized = *checked;
                                cx.notify();
                            }
                        });
                    }),
            )
            .child(
                div()
                    .text_xs()
                    .text_color(if reject_unauthorized {
                        cx.theme().muted_foreground
                    } else {
                        cx.theme().foreground
                    })
                    .child(if reject_unauthorized {
                        "Reject unknown, expired, and hostname-mismatched certificates."
                    } else {
                        "Warning: TLS remains encrypted, but the server identity will not be verified."
                    }),
            )
            .child(self.field(
                "Additional CA certificate (optional PEM path)",
                &self.ca_certificate_path,
                "/path/to/rds-ca.pem",
            ))
    }
}
