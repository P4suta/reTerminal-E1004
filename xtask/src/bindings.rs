use e1004_bsp::pins::*;
use std::collections::BTreeMap;
use syn::{
    visit::{self, Visit},
    Expr, Item, Pat,
};
const DIGITAL_EXTENSION_PINS: usize = 4;
const EXTENSION_ANALOG_INDEX: usize = DIGITAL_EXTENSION_PINS;

#[derive(Default)]
struct Pins(Vec<String>);
impl<'ast> Visit<'ast> for Pins {
    fn visit_expr_field(&mut self, field: &'ast syn::ExprField) {
        if matches!(&*field.base, Expr::Path(path) if path.path.is_ident("p")) {
            if let syn::Member::Named(name) = &field.member {
                self.0.push(name.to_string());
            }
        }
        visit::visit_expr_field(self, field);
    }
}
#[derive(Default)]
struct Levels(Vec<String>);
impl<'ast> Visit<'ast> for Levels {
    fn visit_expr_path(&mut self, path: &'ast syn::ExprPath) {
        if path.path.segments.len() == 2 && path.path.segments[0].ident == "Level" {
            self.0.push(path.path.segments[1].ident.to_string());
        }
        visit::visit_expr_path(self, path);
    }
}
fn levels(expression: &Expr) -> Vec<String> {
    let mut found = Levels::default();
    found.visit_expr(expression);
    found.0
}
fn pins(expression: &Expr) -> Vec<String> {
    let mut found = Pins::default();
    found.visit_expr(expression);
    found.0
}

#[derive(Default)]
struct Bindings {
    locals: BTreeMap<String, Vec<String>>,
    fields: BTreeMap<String, Vec<String>>,
    measurement: Vec<Vec<String>>,
    duplicate: bool,
    methods: BTreeMap<String, Vec<Vec<String>>>,
    local_levels: BTreeMap<String, Vec<String>>,
    field_levels: BTreeMap<String, Vec<String>>,
}
impl<'ast> Visit<'ast> for Bindings {
    fn visit_local(&mut self, local: &'ast syn::Local) {
        if let (Pat::Ident(name), Some(value)) = (&local.pat, &local.init) {
            self.duplicate |= self
                .locals
                .insert(name.ident.to_string(), pins(&value.expr))
                .is_some();
            self.local_levels
                .insert(name.ident.to_string(), levels(&value.expr));
        }
        visit::visit_local(self, local);
    }
    fn visit_expr_struct(&mut self, value: &'ast syn::ExprStruct) {
        if value
            .path
            .segments
            .last()
            .is_some_and(|part| part.ident == "Hardware")
        {
            for field in &value.fields {
                if let syn::Member::Named(name) = &field.member {
                    self.duplicate |= self
                        .fields
                        .insert(name.to_string(), pins(&field.expr))
                        .is_some();
                    self.field_levels
                        .insert(name.to_string(), levels(&field.expr));
                }
            }
        }
        visit::visit_expr_struct(self, value);
    }
    fn visit_expr_method_call(&mut self, call: &'ast syn::ExprMethodCall) {
        let pins = call.args.iter().flat_map(pins).collect::<Vec<_>>();
        if !pins.is_empty() {
            self.methods
                .entry(call.method.to_string())
                .or_default()
                .push(pins);
        }
        visit::visit_expr_method_call(self, call);
    }
    fn visit_expr_call(&mut self, call: &'ast syn::ExprCall) {
        if matches!(&*call.func,Expr::Path(path) if path.path.segments.last().is_some_and(|part| part.ident=="measurements"))
        {
            self.measurement
                .push(call.args.iter().flat_map(pins).collect());
        }
        visit::visit_expr_call(self, call);
    }
}
fn gpio(numbers: &[u8]) -> Vec<String> {
    numbers
        .iter()
        .map(|number| format!("GPIO{number}"))
        .collect()
}
fn peripheral(name: &str, numbers: &[u8]) -> Vec<String> {
    std::iter::once(name.to_owned())
        .chain(gpio(numbers))
        .collect()
}

pub fn validate(source: &str) -> Result<(), &'static str> {
    let file = syn::parse_file(source).map_err(|_| "invalid firmware Rust syntax")?;
    let mut mains = file.items.iter().filter_map(|item| match item {
        Item::Fn(function) if function.sig.ident == "main" => Some(function),
        _ => None,
    });
    let main = mains.next().ok_or("missing firmware entry point")?;
    if mains.next().is_some() {
        return Err("duplicate firmware entry point");
    }
    let mut actual = Bindings::default();
    actual.visit_item_fn(main);
    for (method, pin) in [
        ("with_sck", PANEL_SCK),
        ("with_mosi", PANEL_MOSI),
        ("with_miso", PANEL_MISO),
        ("with_sda", I2C_SDA),
        ("with_scl", I2C_SCL),
        ("with_tx", SERIAL_TX),
        ("with_rx", SERIAL_RX),
    ] {
        if actual.methods.get(method) != Some(&vec![gpio(&[pin])]) {
            return Err("firmware signal role differs from the BSP");
        }
    }
    for (local, level) in [
        ("battery_enable", "Low"),
        ("led", "High"),
        ("buzzer", "Low"),
    ] {
        if actual.local_levels.get(local) != Some(&vec![level.to_owned()]) {
            return Err("incorrect initial output polarity");
        }
    }
    for (field, level) in [
        ("left", "High"),
        ("right", "High"),
        ("dc", "High"),
        ("reset", "High"),
        ("enable", "High"),
        ("sd_cs", "High"),
        ("sd_enable", "Low"),
    ] {
        if actual.field_levels.get(field) != Some(&vec![level.to_owned()]) {
            return Err("incorrect initial panel polarity");
        }
    }
    let mut extension = gpio(&EXTENSION[..DIGITAL_EXTENSION_PINS]);
    extension.extend(["I2C1".to_owned(), "UART1".to_owned()]);
    for (name, expected) in [
        (
            "spi",
            peripheral("SPI2", &[PANEL_SCK, PANEL_MOSI, PANEL_MISO]),
        ),
        ("i2c", peripheral("I2C0", &[I2C_SDA, I2C_SCL])),
        ("uart", peripheral("UART0", &[SERIAL_TX, SERIAL_RX])),
        ("buttons", gpio(&BUTTONS)),
        ("adc", vec!["ADC1".to_owned()]),
        ("battery_pin", gpio(&[BATTERY_ADC])),
        ("battery_enable", gpio(&[BATTERY_ENABLE])),
        ("led", gpio(&[LED])),
        ("buzzer", gpio(&[BUZZER])),
        ("_extension", extension),
    ] {
        if actual.locals.get(name) != Some(&expected) {
            return Err("firmware local binding differs from the BSP");
        }
    }
    for (name, pin) in [
        ("left", PANEL_LEFT_CS),
        ("right", PANEL_RIGHT_CS),
        ("dc", PANEL_DC),
        ("reset", PANEL_RESET),
        ("enable", PANEL_ENABLE),
        ("busy", PANEL_BUSY),
        ("sd_cs", SD_CS),
        ("sd_enable", SD_ENABLE),
        ("sd_detect", SD_DETECT),
    ] {
        if actual.fields.get(name) != Some(&gpio(&[pin])) {
            return Err("firmware panel binding differs from the BSP");
        }
    }
    if actual.duplicate || actual.measurement != [gpio(&[EXTENSION[EXTENSION_ANALOG_INDEX]])] {
        return Err("ambiguous or missing firmware binding");
    }
    Ok(())
}
