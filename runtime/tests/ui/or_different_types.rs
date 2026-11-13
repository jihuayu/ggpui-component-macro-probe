// error: E0308
use probe_ui::prelude::*;
#[derive(Clone, PartialEq)]
enum Either { Number(u32), Name(String) }
#[component]
fn Bad() -> View {
    let source = use_state(|| Either::Number(1));
    view! { match source.get() {
        Either::Number(value) | Either::Name(value) => Text(value),
    } }
}
