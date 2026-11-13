use probe_ui::prelude::*;
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

#[derive(Clone, PartialEq)]
struct Report {
    id: u64,
    name: String,
}
#[derive(Clone, PartialEq)]
enum Phase {
    Idle,
    Ready { name: String },
}
#[derive(Clone, PartialEq)]
struct Reports {
    rows: Vec<Report>,
    phase: Phase,
}
struct ReportStore;
impl StoreKey for ReportStore {
    type State = Reports;
}
struct Publish(String);
impl Command for Publish {
    type Output = ();
    type Error = ();
}

// Test-driver ports. Real widgets would use these handles from UI events.
#[derive(Clone, Default)]
struct Controls {
    expanded: Rc<RefCell<Option<Signal<bool>>>>,
    cached: Rc<RefCell<Option<Signal<bool>>>>,
    detail: Rc<RefCell<Option<Signal<i32>>>>,
    preview: Rc<RefCell<Option<Signal<i32>>>>,
    rows: Rc<RefCell<BTreeMap<u64, Signal<i32>>>>,
    publish: Rc<RefCell<Option<CommandHandle<Publish>>>>,
}

#[component]
fn Details() -> View {
    let controls = use_context::<Controls>();
    let local = use_state(|| 0);
    *controls.detail.borrow_mut() = Some(local);
    view! { Text(format!("if detail = {}", local.get())) }
}
#[component]
fn Preview() -> View {
    let controls = use_context::<Controls>();
    let local = use_state(|| 0);
    *controls.preview.borrow_mut() = Some(local);
    view! { Text(format!("cached preview = {}", local.get())) }
}
#[component]
fn ReportRow(report: ReadProp<Report>) -> View {
    let controls = use_context::<Controls>();
    let note = use_state(|| 0);
    controls.rows.borrow_mut().insert(report.get().id, note);
    view! { Text(format!(
        "row {}: {} / local={}",
        report.get().id, report.get().name, note.get(),
    )) }
}

#[component]
fn ReportPage() -> View {
    let store = use_store::<ReportStore>();
    let controls = use_context::<Controls>();
    let expanded = use_state(|| true);
    let cached = use_state(|| true);
    *controls.expanded.borrow_mut() = Some(expanded);
    *controls.cached.borrow_mut() = Some(cached);
    *controls.publish.borrow_mut() = Some(use_command::<Publish>());
    let phase = store.select(|s| s.phase.clone());
    let rows = store.select(|s| s.rows.clone());
    view! {
        Column {
            Text("ReportPage"),
            if expanded.get() { Details() } else { Text("detail unmounted") },
            KeepAlive(max_entries: 1) {
                if cached.get() { Preview() }
            },
            match phase.get() {
                Phase::Idle => Text("status: idle"),
                Phase::Ready { name } => Text(format!("status: {}", name.get())),
            },
            for report in rows.get() key report.id {
                ReportRow(report: report)
            },
        }
    }
}

fn show(app: &Engine, title: &str) {
    app.flush().unwrap();
    println!("\n{title}");
    for line in app.text() {
        println!("  {line}");
    }
}

fn main() {
    let app = Engine::new();
    let writer = app.store::<ReportStore>(Reports {
        rows: vec![
            Report {
                id: 1,
                name: "Sales".into(),
            },
            Report {
                id: 2,
                name: "Inventory".into(),
            },
        ],
        phase: Phase::Idle,
    });
    // Synchronous adapter: this is not the future async Command executor.
    app.command::<Publish>(move |Publish(name)| {
        writer.update(|s| s.phase = Phase::Ready { name });
        Ok(())
    });
    let controls = Controls::default();
    app.provide(controls.clone());
    app.mount(ReportPage).unwrap();
    show(&app, "1. Initial mount");

    let detail = controls.detail.borrow().unwrap();
    let preview = controls.preview.borrow().unwrap();
    detail.set(7);
    preview.set(8);
    controls.rows.borrow()[&1].set(9);
    show(&app, "2. Change local state, without rebuilding ReportPage");

    controls.expanded.borrow().unwrap().set(false);
    controls.cached.borrow().unwrap().set(false);
    show(&app, "3. Hide ordinary and cached branches");
    assert_eq!(detail.try_get(), Err(Error::Disposed));
    assert_eq!(preview.try_get(), Err(Error::Inactive));

    controls.expanded.borrow().unwrap().set(true);
    controls.cached.borrow().unwrap().set(true);
    show(&app, "4. Reopen: ordinary=0, cached=8");

    writer.update(|s| {
        s.rows[0].name = "Sales revised".into();
        s.rows.reverse();
    });
    show(&app, "5. Reorder + rename: local=9 follows report id=1");

    controls
        .publish
        .borrow()
        .as_ref()
        .unwrap()
        .send(Publish("First".into()))
        .unwrap();
    show(&app, "6. Command -> Store -> match Ready arm");
    controls
        .publish
        .borrow()
        .as_ref()
        .unwrap()
        .send(Publish("Second".into()))
        .unwrap();
    show(&app, "7. Same Ready arm, new name (not a stale snapshot)");

    let before = app.text();
    writer.update(|s| s.rows.push(s.rows[0].clone()));
    assert!(matches!(app.flush(), Err(Error::DuplicateKey(_))));
    assert_eq!(app.text(), before);
    println!("\n8. Duplicate key rejected; previous list preserved");

    assert_eq!(app.stats().mounts["ReportPage"], 1);
    println!("\nComponent mount counts: {:?}", app.stats().mounts);
    println!("PASS: reactive control flow, keys, state lifetimes, Store/Command injection");
}
