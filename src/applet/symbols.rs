use cosmic::widget;

macro_rules! bundled {
    ($($name:ident => $file:literal,)*) => {
        $(
            pub fn $name() -> widget::icon::Handle {
                widget::icon::from_svg_bytes(
                    include_bytes!(concat!("../../resources/icons/", $file)).as_slice(),
                )
                .symbolic(true)
            }
        )*
    };
}

bundled! {
    appearance => "appearance-symbolic.svg",
    button => "button-symbolic.svg",
    code => "code-symbolic.svg",
    droplet => "droplet-symbolic.svg",
    empty => "empty-symbolic.svg",
    eye => "eye-symbolic.svg",
    grid => "grid-symbolic.svg",
    grip => "grip-symbolic.svg",
    hub => "../io.github.meowsystem.Bandeja-symbolic.svg",
    info => "info-symbolic.svg",
    link => "link-symbolic.svg",
    mouse => "mouse-symbolic.svg",
    palette => "palette-symbolic.svg",
    person => "person-symbolic.svg",
    pointer => "pointer-symbolic.svg",
    side => "side-symbolic.svg",
}

macro_rules! named {
    ($($name:ident => $icon:literal,)*) => {
        $(
            pub fn $name() -> widget::icon::Handle {
                widget::icon::from_name($icon).handle()
            }
        )*
    };
}

named! {
    settings => "preferences-system-symbolic",
}
