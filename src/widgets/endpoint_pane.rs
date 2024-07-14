// Copyright 2024 the Cartero authors
//
// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.
//
// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.
//
// You should have received a copy of the GNU General Public License
// along with this program.  If not, see <https://www.gnu.org/licenses/>.
//
// SPDX-License-Identifier: GPL-3.0-or-later

use glib::{subclass::types::ObjectSubclassIsExt, Object};
use gtk::glib;

use crate::{entities::EndpointData, error::CarteroError};

mod imp {
    use std::time::Instant;

    use adw::subclass::breakpoint_bin::BreakpointBinImpl;
    use glib::subclass::InitializingObject;
    use gtk::gio::SettingsBindFlags;
    use gtk::subclass::prelude::*;
    use gtk::{prelude::*, CompositeTemplate, StringObject, WrapMode};
    use isahc::RequestExt;
    use sourceview5::prelude::BufferExt;
    use sourceview5::StyleSchemeManager;

    use crate::app::CarteroApplication;
    use crate::client::{BoundRequest, RequestError};
    use crate::entities::{EndpointData, KeyValue, RequestMethod};
    use crate::error::CarteroError;
    use crate::objects::KeyValueItem;
    use crate::widgets::{KeyValuePane, ResponsePanel};

    #[derive(CompositeTemplate, Default)]
    #[template(resource = "/es/danirod/Cartero/endpoint_pane.ui")]
    pub struct EndpointPane {
        #[template_child(id = "send")]
        pub send_button: TemplateChild<gtk::Button>,

        #[template_child]
        pub header_pane: TemplateChild<KeyValuePane>,

        #[template_child]
        pub variable_pane: TemplateChild<KeyValuePane>,

        #[template_child(id = "method")]
        pub request_method: TemplateChild<gtk::DropDown>,

        #[template_child(id = "url")]
        pub request_url: TemplateChild<gtk::Entry>,

        #[template_child]
        pub request_body: TemplateChild<sourceview5::View>,

        #[template_child]
        pub response: TemplateChild<ResponsePanel>,

        #[template_child]
        pub verbs_string_list: TemplateChild<gtk::StringList>,

        #[template_child]
        pub paned: TemplateChild<gtk::Paned>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for EndpointPane {
        const NAME: &'static str = "CarteroEndpointPane";
        type Type = super::EndpointPane;
        type ParentType = adw::BreakpointBin;

        fn class_init(klass: &mut Self::Class) {
            klass.bind_template();
            klass.bind_template_callbacks();
        }

        fn instance_init(obj: &InitializingObject<Self>) {
            obj.init_template();
        }
    }

    impl ObjectImpl for EndpointPane {
        fn constructed(&self) {
            self.parent_constructed();

            self.init_settings();
            self.variable_pane.assert_always_placeholder();
            self.header_pane.assert_always_placeholder();
            self.init_source_view_style();
        }
    }

    impl WidgetImpl for EndpointPane {}

    impl BreakpointBinImpl for EndpointPane {}

    #[gtk::template_callbacks]
    impl EndpointPane {
        fn init_settings(&self) {
            let app = CarteroApplication::get();
            let settings = app.settings();

            settings
                .bind("body-wrap", &*self.request_body, "wrap-mode")
                .flags(SettingsBindFlags::GET)
                .mapping(|variant, _| {
                    let enabled = variant.get::<bool>().expect("The variant is not a boolean");
                    let mode = match enabled {
                        true => WrapMode::Word,
                        false => WrapMode::None,
                    };
                    Some(mode.to_value())
                })
                .build();

            settings
                .bind(
                    "show-line-numbers",
                    &*self.request_body,
                    "show-line-numbers",
                )
                .flags(SettingsBindFlags::GET)
                .build();
            settings
                .bind("auto-indent", &*self.request_body, "auto-indent")
                .flags(SettingsBindFlags::GET)
                .build();
            settings
                .bind(
                    "indent-style",
                    &*self.request_body,
                    "insert-spaces-instead-of-tabs",
                )
                .flags(SettingsBindFlags::GET)
                .mapping(|variant, _| {
                    let mode = variant
                        .get::<String>()
                        .expect("The variant is not a string");
                    let use_spaces = mode == "spaces";
                    Some(use_spaces.to_value())
                })
                .build();
            settings
                .bind("tab-width", &*self.request_body, "tab-width")
                .flags(SettingsBindFlags::GET)
                .mapping(|variant, _| {
                    let width = variant.get::<String>().unwrap_or("4".into());
                    let value = width.parse::<i32>().unwrap_or(4);
                    Some(value.to_value())
                })
                .build();
            settings
                .bind("tab-width", &*self.request_body, "indent-width")
                .flags(SettingsBindFlags::GET)
                .mapping(|variant, _| {
                    let width = variant.get::<String>().unwrap_or("4".into());
                    let value = width.parse::<i32>().unwrap_or(4);
                    Some(value.to_value())
                })
                .build();

            settings
                .bind("paned-position", &*self.paned, "position")
                .build();
        }

        fn update_source_view_style(&self) {
            let dark_mode = adw::StyleManager::default().is_dark();
            let color_theme = if dark_mode { "Adwaita-dark" } else { "Adwaita" };
            let theme = StyleSchemeManager::default().scheme(color_theme);

            let buffer = self
                .request_body
                .buffer()
                .downcast::<sourceview5::Buffer>()
                .unwrap();
            match theme {
                Some(theme) => {
                    buffer.set_style_scheme(Some(&theme));
                    buffer.set_highlight_syntax(true);
                }
                None => {
                    buffer.set_highlight_syntax(false);
                }
            }
        }
        fn init_source_view_style(&self) {
            self.update_source_view_style();
            adw::StyleManager::default().connect_dark_notify(
                glib::clone!(@weak self as panel => move |_| {
                    panel.update_source_view_style();
                }),
            );
        }

        /// Syncs whether the Send button can be clicked based on whether the request is formed.
        ///
        /// For a request to be formed, an URL has to be set. You cannot submit a request if
        /// you haven't introduced an URL into the corresponding entry field. Every other field
        /// can be blank.
        fn update_send_button_sensitivity(&self) {
            let empty = self.request_url.buffer().text().is_empty();
            self.send_button.set_sensitive(!empty);
        }

        #[template_callback]
        fn on_url_changed(&self) {
            self.update_send_button_sensitivity();
        }

        /// Decodes the HTTP method that has been picked by the user in the dropdown.
        fn request_method(&self) -> RequestMethod {
            let method = self
                .request_method
                .selected_item()
                .unwrap()
                .downcast::<StringObject>()
                .unwrap()
                .string();
            // Note: we should probably be safe from unwrapping here, since it would
            // be impossible to have a method that is not an acceptable value without
            // completely hacking and wrecking the user interface.
            RequestMethod::try_from(method.as_str()).unwrap()
        }

        /// Sets the currently picked HTTP method for the method dropdown.
        ///
        /// TODO: This method should probably be part of its own widget.
        fn set_request_method(&self, rm: RequestMethod) {
            let verb_to_find = String::from(rm);
            let element_count = self.request_method.model().unwrap().n_items();
            let target_position = (0..element_count).find(|i| {
                if let Some(verb) = self.verbs_string_list.string(*i) {
                    if verb == verb_to_find {
                        return true;
                    }
                }
                false
            });
            if let Some(pos) = target_position {
                self.request_method.set_selected(pos);
            }
        }

        /// Sets the value of every widget in the pane into whatever is set by the given endpoint.
        pub fn assign_request(&self, endpoint: &EndpointData) {
            self.request_url.buffer().set_text(endpoint.url.clone());
            self.set_request_method(endpoint.method.clone());

            let headers: Vec<KeyValueItem> = endpoint
                .headers
                .iter()
                .map(|item| KeyValueItem::from(item.clone()))
                .collect();
            let variables: Vec<KeyValueItem> = endpoint
                .variables
                .iter()
                .map(|item| KeyValueItem::from(item.clone()))
                .collect();
            self.header_pane.set_entries(&headers);
            self.variable_pane.set_entries(&variables);
            let body = match &endpoint.body {
                Some(text) => String::from_utf8_lossy(text).into(),
                None => String::default(),
            };
            self.request_body.buffer().set_text(&body);
        }

        /// Takes the current state of the pane and extracts it into an Endpoint value.
        pub(super) fn extract_endpoint(&self) -> Result<EndpointData, CarteroError> {
            let header_list = self.header_pane.get_entries();
            let variable_list = self.variable_pane.get_entries();

            let url = String::from(self.request_url.buffer().text());
            let method = self.request_method();
            let headers = header_list
                .iter()
                .map(|pair| KeyValue {
                    name: pair.header_name(),
                    value: pair.header_value(),
                    active: pair.active(),
                    secret: pair.secret(),
                })
                .collect();
            let variables = variable_list
                .iter()
                .map(|pair| KeyValue {
                    name: pair.header_name(),
                    value: pair.header_value(),
                    active: pair.active(),
                    secret: pair.secret(),
                })
                .collect();

            let body = {
                let buffer = self.request_body.buffer();
                let (start, end) = buffer.bounds();
                let text = buffer.text(&start, &end, true);
                Vec::from(text)
            };
            let body = Some(body);
            Ok(EndpointData {
                url,
                method,
                headers,
                variables,
                body,
            })
        }

        /// Executes an HTTP request based on the current contents of the pane.
        pub(super) async fn perform_request(&self) -> Result<(), CarteroError> {
            let request = self.extract_endpoint()?;
            let request = BoundRequest::try_from(request)?;
            let request_obj = isahc::Request::try_from(request)?;

            let start = Instant::now();
            let mut response_obj = request_obj
                .send_async()
                .await
                .map_err(RequestError::NetworkError)?;
            let response = crate::client::extract_isahc_response(&mut response_obj, &start).await?;
            self.response.assign_from_response(&response);
            Ok(())
        }
    }
}

glib::wrapper! {
    pub struct EndpointPane(ObjectSubclass<imp::EndpointPane>)
        @extends gtk::Widget, gtk::Box;
}

impl Default for EndpointPane {
    fn default() -> Self {
        Object::builder().build()
    }
}

impl EndpointPane {
    /// Updates the contents of the widget so that they reflect the endpoint data.
    ///
    /// TODO: Should enable a binding system?
    pub fn assign_endpoint(&self, endpoint: &EndpointData) {
        let imp = self.imp();
        imp.assign_request(endpoint)
    }

    pub fn extract_endpoint(&self) -> Result<EndpointData, CarteroError> {
        let imp = self.imp();
        imp.extract_endpoint()
    }

    /// Executes an HTTP request based on the current contents of the pane.
    ///
    /// TODO: Should actually the EndpointPane do the requests? This method
    /// will probably change once collections are correctly implemented,
    /// since the EndpointPane would be probably bound to an Endpoint object.
    pub async fn perform_request(&self) -> Result<(), CarteroError> {
        let imp = self.imp();
        imp.response.set_spinning(true);
        let outcome = imp.perform_request().await;
        imp.response.set_spinning(false);
        outcome
    }
}
