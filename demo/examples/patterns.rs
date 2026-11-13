//! cargo run -p probe-demo --example patterns --target-dir target
use probe_ui::prelude::*;
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Clone, PartialEq)]
struct User {
    name: String,
    quota: Option<u32>,
}
#[derive(Clone, PartialEq)]
enum Phase {
    Idle,
    Live { done: u32, total: u32 },
    Cached { done: u32, total: u32 },
    Code(u8),
}
#[derive(Clone, PartialEq)]
struct Model {
    user: Option<User>,
    phase: Phase,
}
struct Reports;
impl StoreKey for Reports {
    type State = Model;
}

struct SetPhase(Phase);
impl Command for SetPhase {
    type Output = ();
    type Error = ();
}
#[derive(Clone, Default)]
struct Controls {
    local: Rc<RefCell<Option<Signal<u32>>>>,
    command: Rc<RefCell<Option<CommandHandle<SetPhase>>>>,
}
#[derive(Clone, Default)]
struct Counters {
    source: Rc<Cell<usize>>,
    guard: Rc<Cell<usize>>,
}
impl Counters {
    fn source(&self, phase: Phase) -> Phase {
        self.source.set(self.source.get() + 1);
        phase
    }
    fn guard(&self, done: u32, total: u32) -> bool {
        self.guard.set(self.guard.get() + 1);
        done <= total
    }
}

#[component]
fn Progress(done: ReadProp<u32>, total: ReadProp<u32>) -> View {
    let note = use_state(|| 0);
    *use_context::<Controls>().local.borrow_mut() = Some(note);
    // Both projections have received the same complete tuple before this memo runs.
    let summary = use_memo(move || format!("{}/{}", done.get(), total.get()));
    view! { Text(format!("progress={}, local={}", summary.get(), note.get())) }
}

#[component]
fn Page() -> View {
    let store = use_store::<Reports>();
    let user = store.select(|model| model.user.clone());
    let phase = store.select(|model| model.phase.clone());
    let counters = use_context::<Counters>();
    *use_context::<Controls>().command.borrow_mut() = Some(use_command::<SetPhase>());
    view! {
        Column {
            if let Some(user) = user.get()
                && let Some(quota) = user.quota
                && quota > 0
            {
                Text(format!("user={}, quota={}", user.get().name, quota.get()))
            } else { Text("guest or quota unavailable") },

            KeepAlive(max_entries: 2) {
                match counters.source(phase.get()) {
                    Phase::Live { done, total } | Phase::Cached { done, total }
                        if counters.guard(done, total) => Progress(done: done, total: total),
                    whole @ Phase::Code(code @ (1 | 3..=7)) =>
                        Text(format!("accepted code={}, tagged={}", code.get(),
                            matches!(whole.get(), Phase::Code(_)))),
                    _ => Text("idle or invalid"),
                }
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
    let writer = app.store::<Reports>(Model {
        user: Some(User {
            name: "Ada".into(),
            quota: Some(10),
        }),
        phase: Phase::Live { done: 1, total: 2 },
    });
    app.command::<SetPhase>(move |SetPhase(phase)| {
        writer.update(|model| model.phase = phase);
        Ok(())
    });
    let controls = Controls::default();
    let counters = Counters::default();
    app.provide(controls.clone());
    app.provide(counters.clone());
    app.mount(Page).unwrap();
    show(&app, "1. let chain + native match selection");
    assert_eq!(counters.source.get(), 1);
    assert_eq!(counters.guard.get(), 1);

    controls.local.borrow().unwrap().set(9);
    show(&app, "2. local update does not rerun parent selector");
    assert_eq!(counters.source.get(), 1);

    let command = controls.command.borrow().as_ref().unwrap().clone();
    command
        .send(SetPhase(Phase::Cached { done: 3, total: 6 }))
        .unwrap();
    show(
        &app,
        "3. Command -> Store -> same or-pattern arm, one tuple publication",
    );
    assert_eq!(app.text()[1], "progress=3/6, local=9");
    assert_eq!(counters.source.get(), 2);
    assert_eq!(counters.guard.get(), 2);
    assert_eq!(app.stats().mounts["Progress"], 1);

    command.send(SetPhase(Phase::Code(5))).unwrap();
    show(
        &app,
        "4. owned @ + nested range/or pattern; progress is cached",
    );
    assert_eq!(
        controls.local.borrow().unwrap().try_get(),
        Err(Error::Inactive)
    );
    assert_eq!(app.text()[1], "accepted code=5, tagged=true");

    writer.update(|model| model.user.as_mut().unwrap().quota = None);
    command
        .send(SetPhase(Phase::Live { done: 4, total: 8 }))
        .unwrap();
    show(
        &app,
        "5. short circuit falls back; cache resumes latest payload and local=9",
    );
    assert_eq!(
        app.text(),
        ["guest or quota unavailable", "progress=4/8, local=9"]
    );
    assert_eq!(app.stats().mounts["Progress"], 1);

    command.send(SetPhase(Phase::Idle)).unwrap();
    show(&app, "6. fallback");
    assert_eq!(app.stats().mounts["Page"], 1);
    println!("\nPASS: if let, let chains, @, or-patterns, unified payload, Store/Command, cache");
}
