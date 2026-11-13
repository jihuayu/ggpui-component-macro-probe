// error: `||` operators are not supported in let chain conditions
use probe_ui::prelude::*;
#[component]
fn Bad() -> View {
    let source = use_state(|| Some(1));
    view! { if let Some(number) = source.get() || true { Text(number) } }
}
