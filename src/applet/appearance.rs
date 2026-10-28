use cosmic::cosmic_config::{Config, ConfigGet, ConfigSet, CosmicConfigEntry, Error};
use serde::{Deserialize, Serialize};

use crate::applet::pins::CONFIG_VERSION;

pub const COLOUR_ICONS_KEY: &str = "colour-icons";
pub const HUB_SIDE_KEY: &str = "hub-side";
pub const HUB_ICON_KEY: &str = "hub-icon";
pub const HIDE_PASSIVE_KEY: &str = "hide-passive";
pub const LEFT_CLICK_KEY: &str = "left-click";

/// Uma escolha de lista suspensa: a ordem de `ALL` é a ordem na tela.
pub trait Choice: Copy + PartialEq + 'static {
    const ALL: &'static [Self];

    fn at(index: usize) -> Option<Self> {
        Self::ALL.get(index).copied()
    }

    fn index(self) -> usize {
        Self::ALL
            .iter()
            .position(|choice| *choice == self)
            .unwrap_or_default()
    }
}

#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub enum HubSide {
    Left,
    #[default]
    Right,
}

impl Choice for HubSide {
    const ALL: &'static [Self] = &[Self::Left, Self::Right];
}

impl HubSide {
    pub fn leads(self) -> bool {
        matches!(self, Self::Left)
    }
}

/// O glifo do botão do painel. `Applet` é o ícone que vem embutido no binário.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub enum HubIcon {
    #[default]
    Applet,
    Grid,
    Dots,
    Arrow,
}

impl Choice for HubIcon {
    const ALL: &'static [Self] = &[Self::Applet, Self::Grid, Self::Dots, Self::Arrow];
}

impl HubIcon {
    /// Nome no tema de ícones; `None` usa o SVG embutido.
    pub fn theme_name(self) -> Option<&'static str> {
        match self {
            Self::Applet => None,
            Self::Grid => Some("view-app-grid-symbolic"),
            Self::Dots => Some("view-more-horizontal-symbolic"),
            Self::Arrow => Some("pan-down-symbolic"),
        }
    }
}

/// O que o clique esquerdo faz num item da bandeja.
#[derive(Clone, Copy, Debug, Default, Deserialize, PartialEq, Eq, Serialize)]
pub enum LeftClick {
    /// Ativa o app; abre o menu se o item só tem menu (`ItemIsMenu`, ou sem `Activate`).
    #[default]
    Auto,
    Activate,
    Menu,
}

impl Choice for LeftClick {
    const ALL: &'static [Self] = &[Self::Auto, Self::Activate, Self::Menu];
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PrimaryAction {
    Activate,
    Menu,
}

impl LeftClick {
    pub fn action(self, item_is_menu: bool, activates: bool) -> PrimaryAction {
        let menu = match self {
            Self::Activate => false,
            Self::Menu => true,
            Self::Auto => item_is_menu || !activates,
        };
        if menu {
            PrimaryAction::Menu
        } else {
            PrimaryAction::Activate
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Appearance {
    colour_icons: bool,
    hub_side: HubSide,
    hub_icon: HubIcon,
    hide_passive: bool,
    left_click: LeftClick,
}

impl Default for Appearance {
    fn default() -> Self {
        Self {
            colour_icons: true,
            hub_side: HubSide::default(),
            hub_icon: HubIcon::default(),
            hide_passive: false,
            left_click: LeftClick::default(),
        }
    }
}

macro_rules! accessors {
    ($($field:ident, $set:ident: $ty:ty;)*) => {
        impl Appearance {
            $(
                pub fn $field(self) -> $ty {
                    self.$field
                }

                pub fn $set(&mut self, value: $ty) {
                    self.$field = value;
                }
            )*
        }
    };
}

accessors! {
    colour_icons, set_colour_icons: bool;
    hub_side, set_hub_side: HubSide;
    hub_icon, set_hub_icon: HubIcon;
    hide_passive, set_hide_passive: bool;
    left_click, set_left_click: LeftClick;
}

/// Lê uma chave; ausente ou ilegível mantém o valor que já estava.
fn read_into<T>(config: &Config, key: &str, slot: &mut T, errors: &mut Vec<Error>) -> bool
where
    T: serde::de::DeserializeOwned + PartialEq,
{
    match config.get::<T>(key) {
        Ok(value) if *slot != value => {
            *slot = value;
            true
        }
        Ok(_) => false,
        Err(error) => {
            errors.push(error);
            false
        }
    }
}

impl Appearance {
    pub const KEYS: [&'static str; 5] = [
        COLOUR_ICONS_KEY,
        HUB_SIDE_KEY,
        HUB_ICON_KEY,
        HIDE_PASSIVE_KEY,
        LEFT_CLICK_KEY,
    ];

    fn read_key(&mut self, config: &Config, key: &str, errors: &mut Vec<Error>) -> bool {
        match key {
            COLOUR_ICONS_KEY => read_into(config, key, &mut self.colour_icons, errors),
            HUB_SIDE_KEY => read_into(config, key, &mut self.hub_side, errors),
            HUB_ICON_KEY => read_into(config, key, &mut self.hub_icon, errors),
            HIDE_PASSIVE_KEY => read_into(config, key, &mut self.hide_passive, errors),
            LEFT_CLICK_KEY => read_into(config, key, &mut self.left_click, errors),
            _ => false,
        }
    }

    fn write_all(self, config: &Config) -> Result<(), Error> {
        config.set(COLOUR_ICONS_KEY, self.colour_icons)?;
        config.set(HUB_SIDE_KEY, self.hub_side)?;
        config.set(HUB_ICON_KEY, self.hub_icon)?;
        config.set(HIDE_PASSIVE_KEY, self.hide_passive)?;
        config.set(LEFT_CLICK_KEY, self.left_click)
    }
}

impl CosmicConfigEntry for Appearance {
    const VERSION: u64 = CONFIG_VERSION;

    fn write_entry(&self, config: &Config) -> Result<(), Error> {
        self.write_all(config)
    }

    fn get_entry(config: &Config) -> Result<Self, (Vec<Error>, Self)> {
        let mut appearance = Self::default();
        let mut errors = Vec::new();
        for key in Self::KEYS {
            appearance.read_key(config, key, &mut errors);
        }

        if errors.is_empty() {
            Ok(appearance)
        } else {
            Err((errors, appearance))
        }
    }

    fn update_keys<T: AsRef<str>>(
        &mut self,
        config: &Config,
        changed_keys: &[T],
    ) -> (Vec<Error>, Vec<&'static str>) {
        let mut errors = Vec::new();
        let mut updated = Vec::new();

        for key in changed_keys {
            let Some(known) = Self::KEYS.iter().find(|known| **known == key.as_ref()) else {
                continue;
            };
            if self.read_key(config, known, &mut errors) {
                updated.push(*known);
            }
        }

        (errors, updated)
    }
}

pub struct AppearanceStore {
    config: Option<Config>,
}

impl AppearanceStore {
    pub fn open(app_id: &str) -> Self {
        match Config::new(app_id, CONFIG_VERSION) {
            Ok(config) => Self {
                config: Some(config),
            },
            Err(error) => {
                tracing::warn!(%error, "appearance will not persist, the config is unavailable");
                Self { config: None }
            }
        }
    }

    pub fn load(&self) -> Appearance {
        let Some(config) = self.config.as_ref() else {
            return Appearance::default();
        };

        let mut appearance = Appearance::default();
        let mut errors = Vec::new();
        for key in Appearance::KEYS {
            appearance.read_key(config, key, &mut errors);
        }
        for error in errors {
            tracing::debug!(%error, "a preference is not stored yet");
        }
        appearance
    }

    pub fn save(&self, appearance: Appearance) {
        let Some(config) = self.config.as_ref() else {
            return;
        };

        if let Err(error) = appearance.write_all(config) {
            tracing::warn!(%error, "could not store the preferences");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_store(name: &str) -> (AppearanceStore, std::path::PathBuf) {
        let path = std::env::temp_dir().join(format!(
            "cosmic-status-hub-appearance-{}-{name}-{:?}",
            std::process::id(),
            std::thread::current().id()
        ));
        let _ = std::fs::remove_dir_all(&path);
        let config = Config::with_custom_path(name, CONFIG_VERSION, path.clone())
            .expect("a config under a temporary path");
        (
            AppearanceStore {
                config: Some(config),
            },
            path,
        )
    }

    #[test]
    fn icons_follow_the_theme_until_the_user_chooses_otherwise() {
        assert!(Appearance::default().colour_icons());
    }

    #[test]
    fn the_hub_button_sits_at_the_right_of_the_strip_until_the_user_moves_it() {
        assert_eq!(Appearance::default().hub_side(), HubSide::Right);
        assert!(!HubSide::Right.leads());
        assert!(HubSide::Left.leads());
    }

    #[test]
    fn every_side_survives_the_trip_through_its_dropdown_index() {
        for side in HubSide::ALL {
            assert_eq!(HubSide::at(side.index()), Some(*side));
        }
        assert_eq!(HubSide::at(HubSide::ALL.len()), None);
        for icon in HubIcon::ALL {
            assert_eq!(HubIcon::at(icon.index()), Some(*icon));
        }
        for click in LeftClick::ALL {
            assert_eq!(LeftClick::at(click.index()), Some(*click));
        }
    }

    #[test]
    fn the_chosen_preferences_survive_a_round_trip() {
        let (store, path) = test_store("appearance-round-trip");
        let mut appearance = Appearance::default();
        appearance.set_colour_icons(false);
        appearance.set_hub_side(HubSide::Left);

        store.save(appearance);

        assert_eq!(store.load(), appearance);
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn a_missing_preference_keeps_icons_adapted_to_the_theme() {
        let (store, path) = test_store("appearance-default");

        assert_eq!(store.load(), Appearance::default());
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn one_stored_preference_does_not_drag_the_other_away_from_its_default() {
        let (store, path) = test_store("appearance-partial");
        let config = store.config.as_ref().unwrap();
        config.set(HUB_SIDE_KEY, HubSide::Left).unwrap();

        let appearance = store.load();

        assert_eq!(appearance.hub_side(), HubSide::Left);
        assert!(
            appearance.colour_icons(),
            "the icon preference was never written, so it keeps its default"
        );
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn a_changed_preference_is_reported_once() {
        let (store, path) = test_store("appearance-change");
        let config = store.config.as_ref().unwrap();
        config.set(COLOUR_ICONS_KEY, false).unwrap();
        let mut appearance = Appearance::default();

        let (errors, changed) = appearance.update_keys(config, &[COLOUR_ICONS_KEY]);

        assert!(errors.is_empty(), "nothing failed to read");
        assert_eq!(changed, [COLOUR_ICONS_KEY]);
        assert!(!appearance.colour_icons());

        let (_, unchanged) = appearance.update_keys(config, &[COLOUR_ICONS_KEY]);
        assert!(unchanged.is_empty(), "an identical reload is not a change");
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn a_changed_side_is_reported_without_touching_the_icon_preference() {
        let (store, path) = test_store("appearance-side-change");
        let config = store.config.as_ref().unwrap();
        config.set(HUB_SIDE_KEY, HubSide::Left).unwrap();
        let mut appearance = Appearance::default();

        let (errors, changed) = appearance.update_keys(config, &[COLOUR_ICONS_KEY, HUB_SIDE_KEY]);

        assert_eq!(
            errors.len(),
            1,
            "only the key that was never written fails to read"
        );
        assert_eq!(changed, [HUB_SIDE_KEY]);
        assert_eq!(appearance.hub_side(), HubSide::Left);
        assert!(appearance.colour_icons());
        let _ = std::fs::remove_dir_all(path);
    }

    #[test]
    fn um_item_so_de_menu_abre_o_menu_no_clique_esquerdo_automatico() {
        let auto = LeftClick::Auto;
        assert_eq!(auto.action(true, true), PrimaryAction::Menu, "ItemIsMenu");
        assert_eq!(
            auto.action(false, false),
            PrimaryAction::Menu,
            "sem Activate"
        );
        assert_eq!(auto.action(false, true), PrimaryAction::Activate);
        assert_eq!(
            LeftClick::Activate.action(true, false),
            PrimaryAction::Activate
        );
        assert_eq!(LeftClick::Menu.action(false, true), PrimaryAction::Menu);
    }

    #[test]
    fn as_preferencias_novas_nascem_como_o_upstream_se_comportava_ou_melhor() {
        let padrao = Appearance::default();
        assert_eq!(padrao.hub_icon(), HubIcon::Applet);
        assert!(!padrao.hide_passive());
        assert_eq!(padrao.left_click(), LeftClick::Auto);
        assert_eq!(HubIcon::Applet.theme_name(), None);
        assert!(HubIcon::ALL[1..].iter().all(|icon| {
            icon.theme_name()
                .is_some_and(|name| name.ends_with("-symbolic"))
        }));
    }

    #[test]
    fn todas_as_preferencias_fazem_a_viagem_de_ida_e_volta() {
        let (store, path) = test_store("appearance-all-keys");
        let mut appearance = Appearance::default();
        appearance.set_colour_icons(false);
        appearance.set_hub_side(HubSide::Left);
        appearance.set_hub_icon(HubIcon::Dots);
        appearance.set_hide_passive(true);
        appearance.set_left_click(LeftClick::Menu);

        store.save(appearance);

        assert_eq!(store.load(), appearance);
        let config = store.config.as_ref().unwrap();
        let mut fresh = Appearance::default();
        let (errors, changed) = fresh.update_keys(config, &Appearance::KEYS);
        assert!(errors.is_empty(), "{errors:?}");
        assert_eq!(changed, Appearance::KEYS);
        assert_eq!(fresh, appearance);
        let _ = std::fs::remove_dir_all(path);
    }
}
