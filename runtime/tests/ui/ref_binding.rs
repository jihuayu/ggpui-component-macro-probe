// error: reactive payload bindings must be owned and immutable (no ref or mut)
use probe_ui::prelude::*;
#[component]
fn Bad() -> View {
    let source = use_state(|| Some(String::from("owned")));
    view! { if let Some(ref name) = source.get() { Text(name) } }
}
