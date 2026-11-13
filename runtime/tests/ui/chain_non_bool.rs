// error: E0308
use probe_ui::prelude::*;
#[component]
fn Bad() -> View {
    let source = use_state(|| Some(1));
    view! { if let Some(number) = source.get() && 42 { Text(number) } }
}
