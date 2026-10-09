//! Shows a tray icon and prints clicks: `cargo run -p fire-ui-tray --example tray`.
use fire_ui_tray::{Event, Icon, Item, MenuItem, Pixmap, Tooltip, Tray};

fn main() -> std::io::Result<()> {
    // A filled orange disc, drawn directly.
    let size = 32;
    let rgba: Vec<u8> = (0..size * size)
        .flat_map(|i| {
            let (x, y) = ((i % size) as f32 - 15.5, (i / size) as f32 - 15.5);
            let a = (12. - (x * x + y * y).sqrt()).clamp(0., 1.);
            [255, 140, 40, (a * 255.) as u8]
        })
        .collect();
    let icon = Icon::Pixels(vec![Pixmap::from_rgba(size, size, &rgba)]);
    let mut item = Item::new("fire-ui-tray-example", "Fire UI tray", icon);
    item.tooltip = Some(Tooltip {
        title: "Fire UI".into(),
        text: "Click me".into(),
    });
    item.menu = vec![
        MenuItem::action("Say hello"),
        MenuItem::Separator,
        MenuItem::action("Quit"),
    ];
    let mut tray = Tray::new(item)?;
    loop {
        match tray.next(None)? {
            Some(Event::Menu(2)) => return Ok(()),
            Some(event) => println!("{event:?}"),
            None => {}
        }
    }
}
