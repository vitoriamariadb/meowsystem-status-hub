pub mod appearance;
pub mod icons;
pub mod identity;
pub mod menu_view;
pub mod message;
pub mod migracao;
pub mod order;
pub mod pins;
pub mod popup;
pub mod subscription;
pub mod symbols;
pub mod tema;
pub mod wayland;

use cosmic::Element;
use cosmic::app::{Core, Task};
use cosmic::cctk::sctk::reexports::calloop;
use cosmic::iced::advanced::text::{Ellipsize, EllipsizeHeightLimit};
use cosmic::iced::clipboard::dnd::DndAction;
use cosmic::iced::clipboard::mime::AsMimeTypes;
use cosmic::iced::mouse::Interaction;
use cosmic::iced::platform_specific::runtime::wayland::popup::SctkPositioner;
use cosmic::iced::platform_specific::shell::commands::popup::destroy_popup;
use cosmic::iced::{Length, Subscription, window};
use cosmic::widget::button::Catalog as _;
use cosmic::widget::{DndSource, Id, dnd_destination, icon, list, mouse_area, text};
use std::borrow::Cow;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use crate::APP_ID;
use crate::applet::appearance::Choice as _;
use crate::applet::icons::{IconCache, TrayIcon, overlay_size};
use crate::applet::message::Message;
use crate::applet::wayland::{ActivateRequest, Raise, TokenRequest, WaylandRequest, WaylandUpdate};
use crate::core::menu::MenuModel;
use crate::core::model::{ItemAddress, TraySnapshot};
use crate::core::{CoreCommand, CoreHandle};
use crate::{fl, links};

const APPEARANCE: usize = 3;
const BEHAVIOUR: usize = 2;
const THEME: usize = 1;
const ABOUT: usize = 2;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PopupState {
    #[default]
    Closed,
    Open {
        tray: window::Id,
        closing: bool,
    },
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PopupBody {
    #[default]
    Items,
    Settings,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum MenuOrigin {
    Hub,
    Panel,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClosedSurface {
    Unknown,
    Tray,
}

const REORDER_MIME: &str = "application/x-cosmic-status-hub-item";

struct ReorderData;

impl AsMimeTypes for ReorderData {
    fn available(&self) -> Cow<'static, [String]> {
        Cow::Owned(vec![REORDER_MIME.to_owned()])
    }

    fn as_bytes(&self, mime_type: &str) -> Option<Cow<'static, [u8]>> {
        (mime_type == REORDER_MIME).then(|| Cow::Borrowed(b"reorder".as_slice()))
    }
}

fn reconcile_surface_closed(id: window::Id, state: &mut PopupState) -> ClosedSurface {
    let PopupState::Open { tray, .. } = *state else {
        return ClosedSurface::Unknown;
    };

    if tray == id {
        *state = PopupState::Closed;
        return ClosedSurface::Tray;
    }

    ClosedSurface::Unknown
}

#[derive(Clone, Debug)]
enum PendingTokenAction {
    Activate(ItemAddress),
    Menu {
        address: ItemAddress,
        id: i32,
        submenu: bool,
        raise: Raise,
    },
}

fn neutral_ink(theme: &cosmic::Theme) -> cosmic::iced::Color {
    tema::paleta()
        .tinta
        .unwrap_or_else(|| theme.cosmic().background(theme.transparent).on.into())
}

fn symbolic_ink() -> cosmic::theme::Svg {
    thread_local! {
        static INK: cosmic::theme::Svg = cosmic::theme::Svg::custom(|theme: &cosmic::Theme| {
            cosmic::iced::widget::svg::Style {
                color: Some(neutral_ink(theme)),
            }
        });
    }

    INK.with(cosmic::theme::Svg::clone)
}

/// Títulos de seção: o acento da extensão, ou a tinta de sempre sem ela.
fn accent_ink() -> cosmic::theme::Svg {
    thread_local! {
        static INK: cosmic::theme::Svg = cosmic::theme::Svg::custom(|theme: &cosmic::Theme| {
            cosmic::iced::widget::svg::Style {
                color: Some(tema::paleta().acento.unwrap_or_else(|| neutral_ink(theme))),
            }
        });
    }

    INK.with(cosmic::theme::Svg::clone)
}

fn neutral_ink_style(
    mut style: cosmic::widget::button::Style,
    theme: &cosmic::Theme,
) -> cosmic::widget::button::Style {
    let ink = neutral_ink(theme);
    style.icon_color = Some(ink);
    style.text_color = Some(tema::paleta().texto.unwrap_or(ink));
    style
}

const HOVER_ALPHA: f32 = 0.25;
const PRESSED_ALPHA: f32 = 0.4;

fn accent_wash(
    mut style: cosmic::widget::button::Style,
    alpha: f32,
) -> cosmic::widget::button::Style {
    if let Some(mut acento) = tema::paleta().acento {
        acento.a *= alpha;
        style.background = Some(cosmic::iced::Background::Color(acento));
    }
    style
}

fn accent_ink_style(
    mut style: cosmic::widget::button::Style,
    theme: &cosmic::Theme,
) -> cosmic::widget::button::Style {
    style = neutral_ink_style(style, theme);
    if let Some(acento) = tema::paleta().acento {
        style.icon_color = Some(acento);
    }
    style
}

fn tray_item_button() -> cosmic::theme::Button {
    cosmic::theme::Button::Custom {
        active: Box::new(|focused, theme| {
            theme.active(focused, false, &cosmic::theme::Button::AppletIcon)
        }),
        disabled: Box::new(|theme| theme.disabled(&cosmic::theme::Button::AppletIcon)),
        hovered: Box::new(|focused, theme| {
            accent_wash(
                theme.hovered(focused, false, &cosmic::theme::Button::AppletIcon),
                HOVER_ALPHA,
            )
        }),
        pressed: Box::new(|focused, theme| {
            accent_wash(
                theme.pressed(focused, false, &cosmic::theme::Button::AppletIcon),
                PRESSED_ALPHA,
            )
        }),
    }
}

fn settings_gear_button() -> cosmic::theme::Button {
    cosmic::theme::Button::Custom {
        active: Box::new(|focused, theme| {
            accent_ink_style(
                theme.active(focused, false, &cosmic::theme::Button::Icon),
                theme,
            )
        }),
        disabled: Box::new(|theme| theme.disabled(&cosmic::theme::Button::Icon)),
        hovered: Box::new(|focused, theme| {
            accent_wash(
                accent_ink_style(
                    theme.hovered(focused, false, &cosmic::theme::Button::Icon),
                    theme,
                ),
                HOVER_ALPHA,
            )
        }),
        pressed: Box::new(|focused, theme| {
            accent_wash(
                accent_ink_style(
                    theme.pressed(focused, false, &cosmic::theme::Button::Icon),
                    theme,
                ),
                PRESSED_ALPHA,
            )
        }),
    }
}

pub struct StatusHub {
    core: Core,
    tray: CoreHandle,
    snapshot: Arc<TraySnapshot>,
    icons: RefCell<IconCache>,
    popup_state: PopupState,
    body: PopupBody,
    pins: pins::Pins,
    draft: Option<pins::Pins>,
    draft_order: Option<Vec<crate::core::model::ItemKey>>,
    appearance: appearance::Appearance,
    draft_appearance: Option<appearance::Appearance>,
    dragging: Option<crate::core::model::ItemKey>,
    pin_store: pins::PinStore,
    appearance_store: appearance::AppearanceStore,
    tema: tema::Tema,
    menu: Option<Arc<MenuModel>>,
    menu_origin: MenuOrigin,
    panel_menu: Option<window::Id>,
    expanded: Vec<i32>,
    pending_menu: Option<ItemAddress>,
    wayland_tx: Option<calloop::channel::Sender<WaylandRequest>>,
    next_token_request: u64,
    pending_tokens: HashMap<String, PendingTokenAction>,
    icon_retry_attempt: usize,
    icon_retry_nonce: u64,
    pending_icon_retry: Option<u64>,
}

const ICON_RETRY_DELAYS: [Duration; 6] = [
    Duration::from_millis(250),
    Duration::from_secs(1),
    Duration::from_secs(3),
    Duration::from_secs(8),
    Duration::from_secs(20),
    Duration::from_secs(45),
];
impl StatusHub {
    fn item_icon_size(&self) -> u16 {
        self.core.applet.suggested_size(true).0
    }

    fn hub_side(&self) -> appearance::HubSide {
        self.appearance.hub_side()
    }

    fn overlap(&self) -> u16 {
        popup::overlap(
            self.core.applet.suggested_padding(true).0,
            self.core.applet.padding_overlap,
            cosmic::theme::spacing().space_none,
        )
    }

    fn lead(&self) -> u16 {
        popup::lead(self.overlap())
    }

    fn button_size(&self) -> (u16, u16) {
        let (icon_width, icon_height) = self.core.applet.suggested_size(true);
        let (major_padding, minor_padding) = self.core.applet.suggested_padding(true);
        let horizontal = self.core.applet.is_horizontal();
        let (horizontal_padding, vertical_padding) = if horizontal {
            (major_padding, minor_padding)
        } else {
            (minor_padding, major_padding)
        };
        let overlap = self.overlap();

        let width = icon_width.saturating_add(horizontal_padding.saturating_mul(2));
        let height = icon_height.saturating_add(vertical_padding.saturating_mul(2));
        if horizontal {
            (width.saturating_sub(overlap), height)
        } else {
            (width, height.saturating_sub(overlap))
        }
    }

    fn applet_button<'a>(
        &self,
        content: impl Into<Element<'a, Message>>,
    ) -> cosmic::widget::Button<'a, Message> {
        let (width, height) = self.button_size();
        cosmic::widget::button::custom(
            cosmic::widget::layer_container(content).center(Length::Fill),
        )
        .width(Length::Fixed(f32::from(width)))
        .height(Length::Fixed(f32::from(height)))
        .class(cosmic::theme::Button::AppletIcon)
    }

    fn grid_inset(&self) -> u16 {
        let (button_width, _) = self.button_size();
        let glyph_inset = button_width.saturating_sub(self.item_icon_size()) / 2;
        popup::MARGIN_X.saturating_sub(glyph_inset)
    }

    fn tray_geometry(&self) -> popup::GridGeometry {
        let (item_width, item_height) = self.button_size();
        let (_, popup_items) = self.partition_items(&self.pins);

        popup::GridGeometry::calculate(
            popup_items.len(),
            u32::from(item_width),
            u32::from(item_height),
            u32::from(self.tray_spacing()),
            u32::from(popup::tray_grid_width(self.grid_inset())),
            u32::from(popup::MARGIN_Y),
        )
    }

    fn tray_item_count(&self) -> usize {
        self.partition_items(&self.pins).1.len()
    }

    fn tray_icons_width(&self) -> u16 {
        popup::tray_icons_width(
            self.tray_item_count(),
            self.button_size().0,
            self.tray_spacing(),
            self.grid_inset(),
        )
    }

    fn hub_width(&self) -> u16 {
        if self.body == PopupBody::Items && self.menu.is_none() {
            popup::tray_surface_width(
                self.tray_item_count(),
                self.button_size().0,
                self.tray_spacing(),
                self.grid_inset(),
            )
        } else {
            popup::SURFACE_WIDTH
        }
    }

    fn tray_spacing(&self) -> u16 {
        u16::try_from(self.core.applet.spacing)
            .unwrap_or(0)
            .saturating_sub(1)
    }

    fn partition_items(
        &self,
        pins: &pins::Pins,
    ) -> (
        Vec<&crate::core::model::TrayItem>,
        Vec<&crate::core::model::TrayItem>,
    ) {
        let hide_passive = self.appearance.hide_passive();
        pins::partition_for_panel(
            self.snapshot.items.iter().filter(|item| {
                !(hide_passive && item.status == crate::core::model::ItemStatus::Passive)
            }),
            pins,
            popup::panel_item_capacity(
                self.core.applet.suggested_bounds,
                self.button_size(),
                self.lead(),
                self.core.applet.is_horizontal(),
            ),
        )
    }

    /// A extensão de tema pode trocar o ícone de um item pelo `Id` dele.
    fn item_icon(&self, item: &crate::core::model::TrayItem, size: u16) -> Option<TrayIcon> {
        if let Some(name) = self.tema.icone_do_item(&item.id) {
            return Some(TrayIcon {
                primary: icon::from_name(name)
                    .size(size)
                    .symbolic(name.ends_with("-symbolic"))
                    .handle(),
                overlay: None,
            });
        }
        self.icons
            .borrow()
            .item(&item.address, item.generation, size)
    }

    fn hub_glyph(&self, size: u16) -> icon::Handle {
        if let Some(name) = self.tema.icone_botao.as_deref() {
            return icon::from_name(name)
                .size(size)
                .symbolic(name.ends_with("-symbolic"))
                .handle();
        }
        match self.appearance.hub_icon().theme_name() {
            Some(name) => icon::from_name(name).size(size).symbolic(true).handle(),
            None => symbols::hub(),
        }
    }

    fn item_button<'a>(
        &'a self,
        item: &'a crate::core::model::TrayItem,
        in_popup: bool,
    ) -> Element<'a, Message> {
        let size = self.item_icon_size();
        let handle = self.item_icon(item, size);

        let button = match handle {
            Some(handle) => self
                .applet_button(Self::item_glyph(handle, size))
                .force_enabled(true),
            None => self
                .applet_button(text(item.label().to_owned()))
                .force_enabled(true),
        };

        let button = if in_popup {
            button.class(tray_item_button())
        } else {
            button
        };

        let address = item.address.clone();

        let button: Element<'a, Message> = if self.is_selected(&item.address) {
            popup::selected_item(button)
        } else {
            button.into()
        };

        let area = mouse_area(button)
            .interaction(Interaction::Pointer)
            .on_press(Message::Activate(address.clone()))
            .on_middle_press(Message::SecondaryActivate(address.clone()))
            .on_right_press(Message::ContextMenu(address));

        area.into()
    }

    fn icon_glyph<'a, M: 'a>(handle: icon::Handle, size: u16) -> Element<'a, M> {
        let size = f32::from(size);
        let symbolic = handle.symbolic;

        cosmic::widget::icon(handle)
            .class(if symbolic {
                symbolic_ink()
            } else {
                cosmic::theme::Svg::default()
            })
            .width(Length::Fixed(size))
            .height(Length::Fixed(size))
            .content_fit(cosmic::iced::ContentFit::Contain)
            .into()
    }

    fn item_glyph<'a, M: 'a>(icon: TrayIcon, size: u16) -> Element<'a, M> {
        let base = Self::icon_glyph(icon.primary, size);
        let Some(overlay) = icon.overlay else {
            return base;
        };
        let badge = cosmic::widget::container(Self::icon_glyph(overlay, overlay_size(size)))
            .width(Length::Fixed(f32::from(size)))
            .height(Length::Fixed(f32::from(size)))
            .align_x(cosmic::iced::Alignment::End)
            .align_y(cosmic::iced::Alignment::End);
        cosmic::iced::widget::Stack::new()
            .push(base)
            .push(badge)
            .into()
    }

    fn drag_grip<'a, M: 'a>() -> Element<'a, M> {
        Self::symbol(symbols::grip(), popup::DRAG_HANDLE_SIZE)
    }

    fn symbol<'a, M: 'a>(handle: icon::Handle, size: u16) -> Element<'a, M> {
        cosmic::widget::icon(handle)
            .class(symbolic_ink())
            .size(size)
            .into()
    }

    fn settings_row_content<'a, M: 'a + 'static>(
        handle: TrayIcon,
        size: u16,
        label: String,
        toggler: Element<'a, M>,
    ) -> Element<'a, M> {
        cosmic::widget::row::with_children(vec![
            Self::drag_grip(),
            Self::item_glyph(handle, size),
            text::body(label)
                .width(Length::Fill)
                .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)))
                .into(),
            toggler,
        ])
        .align_y(cosmic::iced::Alignment::Center)
        .spacing(cosmic::theme::spacing().space_xs)
        .into()
    }

    fn settings_card<'a>() -> list::ListColumn<'a, Message> {
        list::list_column().list_item_padding(popup::list_row_padding(popup::list_row_spacing()))
    }

    fn settings_section(
        handle: icon::Handle,
        title: String,
        card: list::ListColumn<'_, Message>,
    ) -> Element<'_, Message> {
        let heading = cosmic::widget::row::with_children(vec![
            cosmic::widget::icon(handle)
                .class(accent_ink())
                .size(popup::SECTION_ICON)
                .into(),
            text::heading(title).into(),
        ])
        .align_y(cosmic::iced::Alignment::Center)
        .spacing(cosmic::theme::spacing().space_xxs);

        cosmic::widget::column::with_children(vec![heading.into(), card.into()])
            .spacing(popup::section_header_spacing())
            .into()
    }

    fn settings_toggle_row<'a>(
        handle: icon::Handle,
        label: String,
        toggled: bool,
        on_toggle: impl Fn(bool) -> Message + 'static,
    ) -> list::ListButton<'a, Message> {
        let on_press = on_toggle(!toggled);
        let row = cosmic::widget::row::with_children(vec![
            Self::symbol(handle, popup::ROW_ICON),
            text::body(label)
                .width(Length::Fill)
                .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(2)))
                .into(),
            cosmic::widget::toggler(toggled)
                .width(Length::Shrink)
                .on_toggle(on_toggle)
                .into(),
        ])
        .align_y(cosmic::iced::Alignment::Center)
        .spacing(cosmic::theme::spacing().space_xs);

        list::button(row).on_press(on_press)
    }

    fn settings_dropdown_row<'a>(
        handle: icon::Handle,
        label: String,
        options: Vec<String>,
        selected: usize,
        on_select: impl Fn(usize) -> Message + Send + Sync + 'static,
    ) -> Element<'a, Message> {
        cosmic::widget::row::with_children(vec![
            Self::symbol(handle, popup::ROW_ICON),
            text::body(label)
                .width(Length::Fill)
                .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)))
                .into(),
            cosmic::widget::dropdown(options, Some(selected), on_select)
                .padding(popup::dropdown_padding())
                .into(),
        ])
        .align_y(cosmic::iced::Alignment::Center)
        .spacing(cosmic::theme::spacing().space_xs)
        .into()
    }

    fn hub_side_label(side: appearance::HubSide) -> String {
        match side {
            appearance::HubSide::Left => fl!("hub-side-left"),
            appearance::HubSide::Right => fl!("hub-side-right"),
        }
    }

    fn hub_icon_label(choice: appearance::HubIcon) -> String {
        match choice {
            appearance::HubIcon::Applet => fl!("hub-icon-applet"),
            appearance::HubIcon::Grid => fl!("hub-icon-grid"),
            appearance::HubIcon::Dots => fl!("hub-icon-dots"),
            appearance::HubIcon::Arrow => fl!("hub-icon-arrow"),
        }
    }

    fn left_click_label(choice: appearance::LeftClick) -> String {
        match choice {
            appearance::LeftClick::Auto => fl!("left-click-auto"),
            appearance::LeftClick::Activate => fl!("left-click-activate"),
            appearance::LeftClick::Menu => fl!("left-click-menu"),
        }
    }

    fn labels<T: appearance::Choice>(label: fn(T) -> String) -> Vec<String> {
        T::ALL.iter().copied().map(label).collect()
    }

    fn status_row<'a>(handle: icon::Handle, label: String, value: String) -> Element<'a, Message> {
        cosmic::widget::row::with_children(vec![
            Self::symbol(handle, popup::ROW_ICON),
            text::body(label)
                .width(Length::Fill)
                .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)))
                .into(),
            text::caption(value).into(),
        ])
        .align_y(cosmic::iced::Alignment::Center)
        .spacing(cosmic::theme::spacing().space_xs)
        .into()
    }

    fn empty_row<'a>() -> Element<'a, Message> {
        cosmic::widget::row::with_children(vec![
            Self::symbol(symbols::empty(), popup::ROW_ICON),
            text::body(fl!("empty-state"))
                .width(Length::Fill)
                .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(2)))
                .into(),
        ])
        .align_y(cosmic::iced::Alignment::Center)
        .spacing(cosmic::theme::spacing().space_xs)
        .into()
    }

    fn link_row<'a>(
        handle: icon::Handle,
        label: String,
        url: &'static str,
    ) -> list::ListButton<'a, Message> {
        let row = cosmic::widget::row::with_children(vec![
            Self::symbol(handle, popup::ROW_ICON),
            text::body(label)
                .width(Length::Fill)
                .ellipsize(Ellipsize::End(EllipsizeHeightLimit::Lines(1)))
                .into(),
            Self::symbol(symbols::link(), popup::ROW_ICON),
        ])
        .align_y(cosmic::iced::Alignment::Center)
        .spacing(cosmic::theme::spacing().space_xs);

        list::button(row).on_press(Message::OpenLink(url))
    }

    fn settings_drag_preview(
        handle: TrayIcon,
        size: u16,
        label: String,
        pinned: bool,
    ) -> Element<'static, ()> {
        let content = Self::settings_row_content(
            handle,
            size,
            label,
            cosmic::widget::toggler(pinned).width(Length::Shrink).into(),
        );

        cosmic::widget::container(content)
            .width(Length::Fill)
            .padding(popup::list_row_padding(popup::list_row_spacing()))
            .align_y(cosmic::iced::Alignment::Center)
            .class(cosmic::theme::Container::List)
            .into()
    }

    fn is_selected(&self, address: &ItemAddress) -> bool {
        self.pending_menu.as_ref() == Some(address)
            || self
                .menu
                .as_ref()
                .is_some_and(|menu| &menu.owner == address)
    }

    fn hub_layout(&self) -> popup::HubLayout {
        let body = match self.body {
            PopupBody::Items => self.items_height(),
            PopupBody::Settings => popup::settings_body_height(
                &[
                    APPEARANCE,
                    BEHAVIOUR,
                    self.settings_rows().len(),
                    THEME,
                    ABOUT,
                ],
                popup::list_row_spacing(),
                popup::MARGIN_Y,
            ),
        };

        let (header, separator) = match self.body {
            PopupBody::Items => (0, 0),
            PopupBody::Settings => (
                popup::header_height(popup::HEADER_CONTROL, popup::MARGIN_Y),
                popup::separator_height(),
            ),
        };
        if self.menu.is_some() && self.menu_origin == MenuOrigin::Hub {
            popup::HubLayout::with_menu(header, separator, body, popup::menu_separator_height())
        } else {
            popup::HubLayout::new(header, separator, body)
        }
    }

    fn items_height(&self) -> u16 {
        popup::tray_row_height(self.tray_geometry().height)
    }

    fn header(&self) -> Element<'_, Message> {
        let action =
            cosmic::widget::button::icon(cosmic::widget::icon::from_name("object-select-symbolic"))
                .class(cosmic::widget::button::ButtonClass::Text)
                .extra_small()
                .label(fl!("save"))
                .padding(popup::LINK_INSET)
                .spacing(popup::LINK_SPACING)
                .height(Length::Fixed(f32::from(popup::HEADER_CONTROL)))
                .on_press(Message::SaveSettings)
                .into();

        popup::header(
            fl!("settings"),
            action,
            popup::header_height(popup::HEADER_CONTROL, popup::MARGIN_Y),
            popup::MARGIN_Y,
            self.menu.is_some().then_some(Message::DismissMenu),
        )
    }

    fn settings_gear<'a>() -> Element<'a, Message> {
        let control = f32::from(popup::HEADER_CONTROL);
        cosmic::widget::button::icon(symbols::settings())
            .class(settings_gear_button())
            .padding(popup::header_icon_padding())
            .width(Length::Fixed(control))
            .height(Length::Fixed(control))
            .on_press(Message::OpenSettings)
            .into()
    }

    fn popup_body(&self) -> Element<'_, Message> {
        match self.body {
            PopupBody::Items => self.items_body(),
            PopupBody::Settings => self.settings_body(),
        }
    }

    fn items_body(&self) -> Element<'_, Message> {
        let (_, popup_items) = self.partition_items(&self.pins);

        if !popup_items.is_empty() {
            self.icons.borrow_mut().refresh(
                &self.snapshot,
                self.item_icon_size(),
                false,
                self.appearance.colour_icons(),
            );
        }
        popup::tray_row(
            popup_items
                .into_iter()
                .map(|item| self.item_button(item, true))
                .collect(),
            Self::settings_gear(),
            self.tray_spacing(),
            self.items_height(),
            self.grid_inset(),
            self.tray_icons_width(),
        )
    }

    fn settings_body(&self) -> Element<'_, Message> {
        self.icons.borrow_mut().refresh(
            &self.snapshot,
            self.item_icon_size(),
            false,
            self.appearance.colour_icons(),
        );
        let rows = self.settings_rows();

        let draft = self.draft_appearance.unwrap_or(self.appearance);
        let appearance = Self::settings_section(
            symbols::appearance(),
            fl!("appearance"),
            Self::settings_card()
                .add(Self::settings_dropdown_row(
                    symbols::button(),
                    fl!("hub-icon"),
                    Self::labels(Self::hub_icon_label),
                    draft.hub_icon().index(),
                    Message::SelectHubIcon,
                ))
                .add(Self::settings_dropdown_row(
                    symbols::side(),
                    fl!("hub-side"),
                    Self::labels(Self::hub_side_label),
                    draft.hub_side().index(),
                    Message::SelectHubSide,
                ))
                .add(Self::settings_toggle_row(
                    symbols::droplet(),
                    fl!("colour-icons"),
                    draft.colour_icons(),
                    Message::ToggleColourIcons,
                )),
        );

        let behaviour = Self::settings_section(
            symbols::pointer(),
            fl!("behaviour"),
            Self::settings_card()
                .add(Self::settings_dropdown_row(
                    symbols::mouse(),
                    fl!("left-click"),
                    Self::labels(Self::left_click_label),
                    draft.left_click().index(),
                    Message::SelectLeftClick,
                ))
                .add(Self::settings_toggle_row(
                    symbols::eye(),
                    fl!("hide-passive"),
                    draft.hide_passive(),
                    Message::ToggleHidePassive,
                )),
        );

        let card = Self::settings_card();
        let card = if rows.is_empty() {
            card.add(Self::empty_row())
        } else {
            rows.into_iter()
                .fold(card, |card, item| card.add(self.settings_row(item)))
        };
        let tray = Self::settings_section(symbols::grid(), fl!("tray-icons"), card);

        let theme = Self::settings_section(
            symbols::palette(),
            fl!("theme"),
            Self::settings_card().add(Self::status_row(
                symbols::palette(),
                fl!("theme-extension"),
                if self.tema.vazio() {
                    fl!("theme-extension-off")
                } else {
                    fl!("theme-extension-on")
                },
            )),
        );

        let about = Self::settings_section(
            symbols::info(),
            fl!("about"),
            Self::settings_card()
                .add(Self::link_row(
                    symbols::code(),
                    fl!("link-repository"),
                    links::REPOSITORY,
                ))
                .add(Self::link_row(
                    symbols::person(),
                    fl!("link-upstream"),
                    links::UPSTREAM,
                )),
        );

        let list =
            cosmic::widget::column::with_children(vec![appearance, behaviour, tray, theme, about])
                .width(Length::Fill)
                .spacing(popup::section_spacing());

        popup::settings_list(list, popup::settings_body_max_height(), popup::MARGIN_Y)
    }

    fn drag_over(&mut self, target: &crate::core::model::ItemKey) -> Task<Message> {
        let Some(dragged) = self.dragging.clone() else {
            return Task::none();
        };
        let Some(draft) = self.draft_order.as_mut() else {
            return Task::none();
        };
        let Some(index) = draft.iter().position(|key| key == target) else {
            return Task::none();
        };

        if order::move_to(draft, &dragged, index) {
            return cosmic::task::message(Message::Relayout);
        }
        Task::none()
    }

    fn settings_rows(&self) -> Vec<&crate::core::model::TrayItem> {
        let Some(draft) = self.draft_order.as_ref() else {
            return self.snapshot.items.iter().collect();
        };

        draft
            .iter()
            .filter_map(|key| self.snapshot.items.iter().find(|item| &item.key == key))
            .collect()
    }

    fn settings_row<'a>(
        &'a self,
        item: &'a crate::core::model::TrayItem,
    ) -> list::ListButton<'a, Message> {
        let key = item.key.clone();
        let pinned = self
            .draft
            .as_ref()
            .unwrap_or(&self.pins)
            .contains(&item.key);

        let label = item.label().to_owned();
        let size = self.item_icon_size().min(popup::SETTINGS_ROW_ICON);
        let handle = self
            .item_icon(item, self.item_icon_size())
            .unwrap_or_else(|| TrayIcon {
                primary: icon::from_name("application-default").size(size).handle(),
                overlay: None,
            });

        let toggle_key = key.clone();
        let row = Self::settings_row_content(
            handle.clone(),
            size,
            label.clone(),
            cosmic::widget::toggler(pinned)
                .width(Length::Shrink)
                .on_toggle(move |_| Message::TogglePin(toggle_key.clone()))
                .into(),
        );

        let source_id = Id::from(format!("status-hub-reorder-source-{key}"));
        let target_id = Id::from(format!("status-hub-reorder-target-{key}"));
        let start_key = key.clone();
        let target_key = key.clone();

        list::button(row)
            .on_press(Message::TogglePin(key))
            .selected(self.dragging.as_ref() == Some(&item.key))
            .with_dnd_source(Box::new(move |element| {
                DndSource::with_id(element, source_id)
                    .action(DndAction::Move)
                    .drag_threshold(6.0)
                    .drag_content(|| Box::new(ReorderData) as Box<dyn AsMimeTypes + Send>)
                    .drag_icon(move |offset| {
                        let preview = Self::settings_drag_preview(
                            handle.clone(),
                            size,
                            label.clone(),
                            pinned,
                        );
                        let state = preview.as_widget().state();
                        (preview, state, offset)
                    })
                    .on_start(Some(Message::DragStart(start_key.clone())))
                    .on_cancel(Some(Message::DragEnd))
                    .on_finish(Some(Message::DragEnd))
            }))
            .with_dnd_destination(Box::new(move |element| {
                dnd_destination(element, vec![Cow::Borrowed(REORDER_MIME)])
                    .id(target_id)
                    .action(DndAction::Move)
                    .preferred_action(DndAction::Move)
                    .on_enter(move |_, _, _| Message::DragOver(target_key.clone()))
                    .on_drop(|_, _| Message::DragEnd)
                    .on_finish(|_, _, _, _, _| Message::DragEnd)
            }))
    }

    fn request_token(&mut self, action: PendingTokenAction) -> Task<Message> {
        let Some(wayland_tx) = self.wayland_tx.clone() else {
            tracing::debug!("no activation token channel is available");
            return self.complete_token_action(action, None);
        };

        self.next_token_request = self.next_token_request.wrapping_add(1);
        let exec = format!("status-hub-action:{}", self.next_token_request);
        self.pending_tokens.insert(exec.clone(), action);
        if wayland_tx
            .send(WaylandRequest::Token(TokenRequest {
                app_id: APP_ID.to_owned(),
                exec: exec.clone(),
            }))
            .is_err()
        {
            let action = self
                .pending_tokens
                .remove(&exec)
                .expect("the pending token action was just inserted");
            tracing::warn!("the activation token channel is closed");
            return self.complete_token_action(action, None);
        }
        Task::none()
    }

    fn on_wayland(&mut self, update: WaylandUpdate) -> Task<Message> {
        match update {
            WaylandUpdate::Init(sender) => {
                self.wayland_tx = Some(sender);
                Task::none()
            }
            WaylandUpdate::Finished => {
                tracing::warn!("the privileged Wayland connection ended");
                self.wayland_tx = None;
                let pending = std::mem::take(&mut self.pending_tokens);
                Task::batch(
                    pending
                        .into_values()
                        .map(|action| self.complete_token_action(action, None)),
                )
            }
            WaylandUpdate::ActivationToken { token, exec } => {
                tracing::debug!(request = %exec, token = token.is_some(), "activation token");
                match self.pending_tokens.remove(&exec) {
                    Some(action) => self.complete_token_action(action, token),
                    None => Task::none(),
                }
            }
        }
    }

    fn cancel_icon_retry(&mut self) {
        self.icon_retry_nonce = self.icon_retry_nonce.wrapping_add(1);
        self.pending_icon_retry = None;
        self.icon_retry_attempt = 0;
    }

    fn schedule_icon_retry(&mut self) -> Task<Message> {
        let Some(&delay) = ICON_RETRY_DELAYS.get(self.icon_retry_attempt) else {
            self.pending_icon_retry = None;
            return Task::none();
        };
        self.icon_retry_attempt += 1;
        self.icon_retry_nonce = self.icon_retry_nonce.wrapping_add(1);
        let nonce = self.icon_retry_nonce;
        self.pending_icon_retry = Some(nonce);
        cosmic::task::future(async move {
            tokio::time::sleep(delay).await;
            Message::RetryIcons(nonce)
        })
    }

    fn refresh_icons(&mut self, retry_fallbacks: bool, restart: bool) -> Task<Message> {
        let unresolved = self.icons.borrow_mut().refresh(
            &self.snapshot,
            self.item_icon_size(),
            retry_fallbacks,
            self.appearance.colour_icons(),
        );
        if !unresolved {
            self.cancel_icon_retry();
            return Task::none();
        }
        if restart {
            self.cancel_icon_retry();
        }
        self.schedule_icon_retry()
    }

    fn on_snapshot(&mut self, snapshot: Arc<TraySnapshot>) -> Task<Message> {
        let emptied = !self.snapshot.items.is_empty() && snapshot.items.is_empty();
        let slots_before = self.panel_slots();
        let active_menu = self
            .menu
            .as_ref()
            .map(|menu| menu.owner.clone())
            .or_else(|| self.pending_menu.clone());
        self.snapshot = snapshot;

        if let Some(draft) = self.draft_order.take() {
            self.draft_order = Some(order::normalise(&draft, &self.snapshot.items));
        }

        let icon_task = self.refresh_icons(false, true);
        let slots_changed = self.panel_slots() != slots_before;
        let active_disappeared = active_menu.is_some_and(|address| {
            !self
                .snapshot
                .items
                .iter()
                .any(|item| item.address == address)
        });
        let menu_task = if active_disappeared || (slots_changed && self.panel_menu.is_some()) {
            self.forget_menu();
            self.menu_origin = MenuOrigin::Hub;
            self.close_panel_menu()
        } else {
            Task::none()
        };

        if emptied && self.popup_state != PopupState::Closed && self.body == PopupBody::Items {
            return Task::batch([icon_task, menu_task, self.close_popup(true)]);
        }
        Task::batch([icon_task, menu_task])
    }

    fn item_at(&self, address: &ItemAddress) -> Option<&crate::core::model::TrayItem> {
        self.snapshot
            .items
            .iter()
            .find(|item| item.address == *address)
    }

    fn takes_activation_token(&self, address: &ItemAddress) -> bool {
        self.item_at(address)
            .is_some_and(|item| item.takes_activation_token)
    }

    fn raise_for_entry(menu: &MenuModel, id: i32) -> Raise {
        match menu.find(id) {
            Some(entry) if entry.toggle.is_some() => Raise::Changed,
            _ => Raise::Minimized,
        }
    }

    fn raise_window_for(&self, address: &ItemAddress, raise: Raise) {
        let Some(wayland_tx) = self.wayland_tx.as_ref() else {
            return;
        };
        let hints = self
            .item_at(address)
            .map(identity::hints)
            .unwrap_or_default();
        if hints.is_empty() {
            tracing::debug!(item = %address, "no window hints for this item");
            return;
        }

        let _ = wayland_tx.send(WaylandRequest::Activate(ActivateRequest { hints, raise }));
    }

    fn complete_token_action(
        &mut self,
        action: PendingTokenAction,
        token: Option<String>,
    ) -> Task<Message> {
        match action {
            PendingTokenAction::Activate(address) => {
                self.raise_window_for(&address, Raise::Unfocused);
                self.tray.send(CoreCommand::Primary { address, token });
                self.close_popup(true)
            }
            PendingTokenAction::Menu {
                address,
                id,
                submenu,
                raise,
            } => {
                if !submenu && !self.takes_activation_token(&address) {
                    self.raise_window_for(&address, raise);
                }
                self.tray.send(CoreCommand::MenuClicked {
                    address,
                    id,
                    token,
                    close: !submenu,
                });
                if submenu {
                    if let Some(position) = self.expanded.iter().position(|open| *open == id) {
                        self.expanded.remove(position);
                    } else {
                        self.expanded.push(id);
                    }
                    Task::none()
                } else if self.menu_origin == MenuOrigin::Panel {
                    self.menu = None;
                    self.expanded.clear();
                    self.menu_origin = MenuOrigin::Hub;
                    self.close_panel_menu()
                } else {
                    self.close_popup(false)
                }
            }
        }
    }

    fn on_panel_menu(&mut self, menu: Option<Arc<MenuModel>>) -> Task<Message> {
        let owner_changed = self
            .menu
            .as_ref()
            .zip(menu.as_ref())
            .is_some_and(|(old, new)| old.owner != new.owner);
        if owner_changed || menu.is_none() {
            self.expanded.clear();
        }

        let slot = menu
            .as_ref()
            .and_then(|model| self.panel_slot_of(&model.owner));
        let showing = self.panel_menu.is_some();
        self.menu = menu;

        let Some(slot) = slot else {
            self.menu = None;
            self.menu_origin = MenuOrigin::Hub;
            return self.close_panel_menu();
        };

        if showing && !owner_changed {
            return cosmic::task::message(Message::Relayout);
        }

        let closed = self.close_panel_menu();
        closed.chain(self.open_panel_menu(slot))
    }

    fn on_menu(&mut self, menu: Option<Arc<MenuModel>>) -> Task<Message> {
        self.pending_menu = None;

        if self.menu_origin == MenuOrigin::Panel {
            return self.on_panel_menu(menu);
        }

        if self.popup_state == PopupState::Closed {
            if menu.is_some() {
                self.tray.send(CoreCommand::CloseMenu);
            }
            self.menu = None;
            self.expanded.clear();
            return Task::none();
        }

        let owner_changed = self
            .menu
            .as_ref()
            .zip(menu.as_ref())
            .is_some_and(|(old, new)| old.owner != new.owner);
        if owner_changed || menu.is_none() {
            self.expanded.clear();
        }

        self.menu = menu;
        cosmic::task::message(Message::Relayout)
    }

    fn close_popup(&mut self, close_menu: bool) -> Task<Message> {
        if close_menu && self.menu.is_some() {
            self.tray.send(CoreCommand::CloseMenu);
        }
        self.expanded.clear();
        self.menu = None;
        self.pending_menu = None;

        match self.popup_state {
            PopupState::Open {
                tray,
                closing: false,
            } => {
                self.popup_state = PopupState::Open {
                    tray,
                    closing: true,
                };
                destroy_popup(tray)
            }
            PopupState::Closed | PopupState::Open { closing: true, .. } => Task::none(),
        }
    }

    fn on_toggle_popup(&mut self) -> Task<Message> {
        if self.popup_state != PopupState::Closed {
            return self.close_popup(true);
        }

        let closed_panel_menu = self.close_panel_menu().chain(self.reload_theme());
        self.menu_origin = MenuOrigin::Hub;
        self.menu = None;
        self.pending_menu = None;
        self.expanded.clear();

        let popup = window::Id::unique();
        self.body = PopupBody::Items;
        self.popup_state = PopupState::Open {
            tray: popup,
            closing: false,
        };

        let parent = self
            .core
            .main_window_id()
            .expect("applet has a main window");
        closed_panel_menu.chain(Self::open_popup_surface(parent, popup))
    }

    fn on_surface_closed(&mut self, id: window::Id) -> Task<Message> {
        if self.panel_menu == Some(id) {
            self.panel_menu = None;
            self.menu_origin = MenuOrigin::Hub;
            self.forget_menu();
            return Task::none();
        }

        match reconcile_surface_closed(id, &mut self.popup_state) {
            ClosedSurface::Tray => {
                self.body = PopupBody::Items;
                let saved = self.commit_settings();
                self.forget_menu();
                return saved;
            }
            ClosedSurface::Unknown => {}
        }
        Task::none()
    }

    fn on_open_settings(&mut self) -> Task<Message> {
        self.body = PopupBody::Settings;
        self.draft = Some(self.pins.clone());
        self.draft_order = Some(order::normalise(&[], &self.snapshot.items));
        self.draft_appearance = Some(self.appearance);
        self.forget_menu();
        cosmic::task::message(Message::Relayout)
    }

    fn on_save_settings(&mut self) -> Task<Message> {
        self.body = PopupBody::Items;
        let saved = self.commit_settings();
        self.forget_menu();
        Task::batch([saved, self.close_popup(true)])
    }

    /// Fechar o popup por fora também guarda: a escolha feita não se perde por
    /// um clique fora dele.
    fn commit_settings(&mut self) -> Task<Message> {
        let draft = self.draft.take();
        let draft_order = self.draft_order.take();
        let draft_appearance = self.draft_appearance.take();
        self.dragging = None;

        if let Some(draft) = draft
            && draft != self.pins
        {
            self.pin_store.save(&draft);
            self.pins = draft;
        }

        if let Some(order) = draft_order {
            self.tray.send(CoreCommand::SetRemembered(order));
        }

        let appearance_task = if let Some(appearance) = draft_appearance
            && appearance != self.appearance
        {
            self.appearance_store.save(appearance);
            self.appearance = appearance;
            self.refresh_icons(false, true)
        } else {
            Task::none()
        };

        appearance_task.chain(cosmic::task::message(Message::Relayout))
    }

    /// Relê a extensão de tema; troca de pack vale no próximo abrir do popup.
    fn reload_theme(&mut self) -> Task<Message> {
        let tema = tema::carregar(APP_ID);
        if tema == self.tema {
            return Task::none();
        }
        tema::vestir(&tema);
        self.tema = tema;
        self.refresh_icons(false, true)
    }

    fn forget_menu(&mut self) {
        self.expanded.clear();
        self.menu = None;
        self.pending_menu = None;
        self.tray.send(CoreCommand::CloseMenu);
    }

    fn close_panel_menu(&mut self) -> Task<Message> {
        match self.panel_menu.take() {
            Some(menu) => destroy_popup(menu),
            None => Task::none(),
        }
    }

    fn open_panel_menu(&mut self, slot: usize) -> Task<Message> {
        let Some(parent) = self.core.main_window_id() else {
            return Task::none();
        };

        let menu = window::Id::unique();
        self.panel_menu = Some(menu);

        cosmic::surface::surface_task(cosmic::surface::action::app_popup::<Self>(
            |_| cosmic::surface::action::LiveSettings::default(),
            move |app| {
                let button = app.button_size();
                let lead = app.lead();
                let horizontal = app.core.applet.is_horizontal();
                let mut settings = app.core.applet.get_popup_settings(
                    parent,
                    menu,
                    Some((u32::from(popup::SURFACE_WIDTH), 1)),
                    None,
                    None,
                );
                settings.positioner.anchor_rect =
                    popup::panel_slot_rect(slot, button, lead, horizontal);
                settings.positioner.size_limits = popup::size_limits();
                settings
            },
            None,
        ))
    }

    fn panel_slot_of(&self, address: &ItemAddress) -> Option<usize> {
        let (pinned, _) = self.partition_items(&self.pins);
        pinned
            .iter()
            .position(|item| &item.address == address)
            .map(|index| popup::pinned_slot(self.hub_side(), index))
    }

    fn panel_slots(&self) -> usize {
        let (panel, _) = self.partition_items(&self.pins);
        panel.len() + 1
    }

    fn on_colour_icons(&mut self, colour_icons: bool) -> Task<Message> {
        let Some(appearance) = self.draft_appearance.as_mut() else {
            return Task::none();
        };
        if appearance.colour_icons() == colour_icons {
            return Task::none();
        }

        appearance.set_colour_icons(colour_icons);
        cosmic::task::message(Message::Relayout)
    }

    fn on_hub_side(&mut self, index: usize) -> Task<Message> {
        let Some(side) = appearance::HubSide::at(index) else {
            return Task::none();
        };
        let Some(appearance) = self.draft_appearance.as_mut() else {
            return Task::none();
        };
        if appearance.hub_side() == side {
            return Task::none();
        }

        appearance.set_hub_side(side);
        cosmic::task::message(Message::Relayout)
    }

    fn edit_appearance(&mut self, edit: impl FnOnce(&mut appearance::Appearance)) -> Task<Message> {
        let Some(appearance) = self.draft_appearance.as_mut() else {
            return Task::none();
        };
        let before = *appearance;
        edit(appearance);
        if *appearance == before {
            return Task::none();
        }
        cosmic::task::message(Message::Relayout)
    }

    fn on_context_menu(&mut self, address: ItemAddress) -> Task<Message> {
        self.menu_origin = if self.panel_slot_of(&address).is_some() {
            MenuOrigin::Panel
        } else {
            MenuOrigin::Hub
        };
        self.pending_menu = self
            .snapshot
            .items
            .iter()
            .find(|item| item.address == address)
            .and_then(|item| item.menu_path.as_ref().map(|_| address.clone()));
        self.tray.send(CoreCommand::Context { address });
        Task::none()
    }

    fn on_activate(&mut self, address: ItemAddress) -> Task<Message> {
        let action = self
            .item_at(&address)
            .map_or(appearance::PrimaryAction::Activate, |item| {
                self.appearance
                    .left_click()
                    .action(item.item_is_menu, item.activates)
            });
        tracing::debug!(item = %address, ?action, "primary click");
        match action {
            appearance::PrimaryAction::Activate => {
                self.request_token(PendingTokenAction::Activate(address))
            }
            appearance::PrimaryAction::Menu => self.on_context_menu(address),
        }
    }

    fn apply_order(&mut self, order: order::Order) -> Task<Message> {
        self.tray
            .send(CoreCommand::SetRemembered(order.into_keys()));
        Task::none()
    }

    fn apply_pins(&mut self, pins: pins::Pins) -> Task<Message> {
        if self.pins == pins {
            return Task::none();
        }

        self.pins = pins;

        let close_menu = if self.menu.is_some() || self.pending_menu.is_some() {
            self.forget_menu();
            self.menu_origin = MenuOrigin::Hub;
            self.close_panel_menu()
        } else {
            Task::none()
        };
        close_menu.chain(cosmic::task::message(Message::Relayout))
    }

    fn apply_appearance(&mut self, appearance: appearance::Appearance) -> Task<Message> {
        if self.appearance == appearance {
            return Task::none();
        }

        self.appearance = appearance;
        self.refresh_icons(false, true)
    }

    fn hub_positioner(&self, parent: window::Id, id: window::Id) -> SctkPositioner {
        let height = self.hub_layout().height();
        let mut settings = self.core.applet.get_popup_settings(
            parent,
            id,
            Some((u32::from(self.hub_width()), u32::from(height))),
            None,
            None,
        );
        settings.positioner.anchor_rect = popup::panel_slot_rect(
            popup::hub_slot(self.hub_side(), self.panel_slots()),
            self.button_size(),
            self.lead(),
            self.core.applet.is_horizontal(),
        );
        settings.positioner.reactive = false;
        settings.positioner.size_limits = popup::size_limits();
        settings.positioner
    }

    fn open_popup_surface(parent: window::Id, id: window::Id) -> Task<Message> {
        cosmic::surface::surface_task(cosmic::surface::action::app_popup::<Self>(
            |_| cosmic::surface::action::LiveSettings::default(),
            move |app| {
                let mut settings = app
                    .core
                    .applet
                    .get_popup_settings(parent, id, None, None, None);
                settings.positioner = app.hub_positioner(parent, id);
                settings
            },
            None,
        ))
    }
}

impl cosmic::Application for StatusHub {
    type Executor = cosmic::SingleThreadExecutor;
    type Flags = CoreHandle;
    type Message = Message;

    const APP_ID: &'static str = APP_ID;

    fn init(core: Core, tray: Self::Flags) -> (Self, Task<Message>) {
        let snapshot = tray.snapshot();
        migracao::migrar(APP_ID, pins::CONFIG_VERSION);
        let tema = tema::carregar(APP_ID);
        tema::vestir(&tema);
        let pin_store = pins::PinStore::open(APP_ID);
        let pins = pin_store.load();
        let appearance_store = appearance::AppearanceStore::open(APP_ID);
        let appearance = appearance_store.load();
        (
            Self {
                core,
                tray,
                snapshot,
                icons: RefCell::new(IconCache::default()),
                popup_state: PopupState::Closed,
                body: PopupBody::Items,
                pins,
                draft: None,
                draft_order: None,
                appearance,
                draft_appearance: None,
                dragging: None,
                pin_store,
                appearance_store,
                tema,
                menu: None,
                menu_origin: MenuOrigin::Hub,
                panel_menu: None,
                expanded: Vec::new(),
                pending_menu: None,
                wayland_tx: None,
                next_token_request: 0,
                pending_tokens: HashMap::new(),
                icon_retry_attempt: 0,
                icon_retry_nonce: 0,
                pending_icon_retry: None,
            },
            Task::none(),
        )
    }

    fn core(&self) -> &Core {
        &self.core
    }

    fn core_mut(&mut self) -> &mut Core {
        &mut self.core
    }

    fn style(&self) -> Option<cosmic::iced::theme::Style> {
        Some(cosmic::applet::style())
    }

    fn subscription(&self) -> Subscription<Message> {
        Subscription::batch([
            subscription::snapshots(&self.tray),
            subscription::menus(&self.tray),
            subscription::pins(),
            subscription::order(),
            subscription::appearance(),
            wayland::subscription().map(Message::Wayland),
        ])
    }

    fn update(&mut self, message: Message) -> Task<Message> {
        match message {
            Message::Relayout => Task::none(),

            Message::Snapshot(snapshot) => self.on_snapshot(snapshot),

            Message::TogglePopup => self.on_toggle_popup(),

            Message::SurfaceClosed(id) => self.on_surface_closed(id),

            Message::Activate(address) => self.on_activate(address),

            Message::SecondaryActivate(address) => {
                self.tray.send(CoreCommand::Secondary { address });
                Task::none()
            }

            Message::ContextMenu(address) => self.on_context_menu(address),

            Message::RetryIcons(nonce) => {
                if self.pending_icon_retry != Some(nonce) {
                    return Task::none();
                }
                self.pending_icon_retry = None;
                self.refresh_icons(true, false)
            }

            Message::Menu(menu) => self.on_menu(menu),

            Message::MenuEntry { id, submenu } => {
                let Some(menu) = self.menu.clone() else {
                    return Task::none();
                };
                let raise = if submenu {
                    Raise::Changed
                } else {
                    Self::raise_for_entry(&menu, id)
                };
                self.request_token(PendingTokenAction::Menu {
                    address: menu.owner.clone(),
                    id,
                    submenu,
                    raise,
                })
            }

            Message::OpenSettings => self.on_open_settings(),

            Message::SaveSettings => self.on_save_settings(),

            Message::AppearanceChanged(appearance) => self.apply_appearance(appearance),

            Message::PinsChanged(pins) => self.apply_pins(pins),

            Message::DismissMenu => {
                if self.menu.is_none() {
                    return Task::none();
                }
                self.forget_menu();
                cosmic::task::message(Message::Relayout)
            }

            Message::OrderChanged(order) => self.apply_order(order),

            Message::DragStart(key) => {
                self.dragging = Some(key);
                Task::none()
            }

            Message::DragOver(target) => self.drag_over(&target),

            Message::DragEnd => {
                self.dragging = None;
                Task::none()
            }

            Message::TogglePin(key) => {
                let Some(draft) = self.draft.as_mut() else {
                    return Task::none();
                };
                draft.toggle(&key);
                cosmic::task::message(Message::Relayout)
            }

            Message::ToggleColourIcons(colour_icons) => self.on_colour_icons(colour_icons),

            Message::SelectHubSide(index) => self.on_hub_side(index),

            Message::SelectHubIcon(index) => match appearance::HubIcon::at(index) {
                Some(choice) => self.edit_appearance(|a| a.set_hub_icon(choice)),
                None => Task::none(),
            },

            Message::SelectLeftClick(index) => match appearance::LeftClick::at(index) {
                Some(choice) => self.edit_appearance(|a| a.set_left_click(choice)),
                None => Task::none(),
            },

            Message::ToggleHidePassive(on) => self.edit_appearance(|a| a.set_hide_passive(on)),

            Message::OpenLink(url) => cosmic::task::future(async move {
                links::open(url).await;
                Message::Relayout
            }),

            Message::Wayland(update) => self.on_wayland(update),
        }
    }

    fn view(&self) -> Element<'_, Message> {
        let horizontal = self.core.applet.is_horizontal();

        let size = self.item_icon_size();
        let hub = self
            .applet_button(Self::icon_glyph(self.hub_glyph(size), size))
            .on_press(Message::TogglePopup);

        let (pinned, _) = self.partition_items(&self.pins);
        if pinned.is_empty() {
            return popup::panel_surface(
                hub,
                self.core.applet.suggested_bounds,
                self.lead(),
                horizontal,
            );
        }

        self.icons.borrow_mut().refresh(
            &self.snapshot,
            self.item_icon_size(),
            false,
            self.appearance.colour_icons(),
        );

        let hub: Element<'_, Message> = hub.into();
        let buttons = pinned.into_iter().map(|item| self.item_button(item, false));
        let slots: Vec<Element<'_, Message>> = if self.hub_side().leads() {
            std::iter::once(hub).chain(buttons).collect()
        } else {
            buttons.chain(std::iter::once(hub)).collect()
        };

        let strip: Element<'_, Message> = if horizontal {
            cosmic::widget::row::with_children(slots)
                .align_y(cosmic::iced::Alignment::Center)
                .into()
        } else {
            cosmic::widget::column::with_children(slots)
                .align_x(cosmic::iced::Alignment::Center)
                .into()
        };

        popup::panel_surface(
            strip,
            self.core.applet.suggested_bounds,
            self.lead(),
            horizontal,
        )
    }

    fn view_window(&self, id: window::Id) -> Element<'_, Message> {
        if self.panel_menu == Some(id) {
            return match self.menu.as_ref() {
                Some(model) => popup::menu_surface(menu_view::view(
                    model,
                    &self.expanded,
                    menu_view::Edges::BOTH,
                )),
                None => text::body("").into(),
            };
        }

        let PopupState::Open { tray, .. } = self.popup_state else {
            return text::body("").into();
        };

        if tray == id {
            let body = if self.menu.is_some() {
                mouse_area(self.popup_body())
                    .on_press(Message::DismissMenu)
                    .into()
            } else {
                self.popup_body()
            };
            let edges = if popup::menu_comes_first(self.core.applet.anchor) {
                menu_view::Edges::TOP
            } else {
                menu_view::Edges::BOTTOM
            };
            let menu = self
                .menu
                .as_ref()
                .map(|model| menu_view::view(model, &self.expanded, edges));
            return popup::hub_surface(
                (self.body == PopupBody::Settings).then(|| self.header()),
                body,
                menu,
                self.hub_layout(),
                self.core.applet.anchor,
                self.hub_width(),
            );
        }

        text::body("").into()
    }

    fn on_close_requested(&self, id: window::Id) -> Option<Message> {
        Some(Message::SurfaceClosed(id))
    }
}

pub fn run(tray: CoreHandle) -> cosmic::iced::Result {
    cosmic::applet::run::<StatusHub>(tray)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn open(tray: window::Id) -> PopupState {
        PopupState::Open {
            tray,
            closing: false,
        }
    }

    #[test]
    fn closing_the_popup_leaves_no_surface() {
        let tray = window::Id::unique();
        let mut state = open(tray);

        assert_eq!(
            reconcile_surface_closed(tray, &mut state),
            ClosedSurface::Tray
        );
        assert_eq!(state, PopupState::Closed);
    }

    #[test]
    fn an_unrelated_surface_does_not_change_popup_state() {
        let tray = window::Id::unique();
        let mut state = open(tray);

        assert_eq!(
            reconcile_surface_closed(window::Id::unique(), &mut state),
            ClosedSurface::Unknown
        );
        assert_eq!(state, open(tray));
    }
}
