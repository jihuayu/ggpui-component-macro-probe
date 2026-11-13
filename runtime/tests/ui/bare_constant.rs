// error: E0308
// error: is interpreted as a constant, not a new binding
use probe_ui::prelude::*;
const FIRST: u8 = 1;
#[component]
fn Bad() -> View {
    let source = use_state(|| 1_u8);
    view! { match source.get() { FIRST => Text(FIRST), _ => Text("other") } }
}
