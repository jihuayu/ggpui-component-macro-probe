// error: component takes no options
use probe_ui::prelude::*;
#[component(unknown)]
fn Bad() -> View { view! { Text("no") } }
