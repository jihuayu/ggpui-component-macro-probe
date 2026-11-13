// error: reactive patterns must be visible to view!
use probe_ui::prelude::*;
macro_rules! binding { () => { Some(number) }; }
#[component]
fn Bad() -> View {
    let source = use_state(|| Some(1));
    view! { if let binding!() = source.get() { Text(number) } }
}
