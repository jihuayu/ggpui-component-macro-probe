// error: reactive payload bindings must be owned and immutable (no ref or mut)
use probe_ui::prelude::*;
#[component]
fn Bad() -> View {
    let source = use_state(|| Some(1));
    view! { if let Some(mut number) = source.get() { Text(number) } }
}
