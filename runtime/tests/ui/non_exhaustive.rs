// error: E0004
use probe_ui::prelude::*;
#[component]
fn Bad() -> View {
    let source = use_state(|| Some(1));
    view! { match source.get() { Some(number) => Text(number) } }
}
