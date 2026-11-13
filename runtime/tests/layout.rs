use probe_ui::prelude::*;

#[test]
fn signal_location_reports_scope_and_layout_dependents() {
    let app = Engine::new();
    let width = app.source(100.0);
    let height = app.source(40.0);
    app.mount(|| {
        View::layout("panel", move || Rect {
            x: 4.0,
            y: 8.0,
            width: width.get(),
            height: height.get(),
        })
    })
    .unwrap();

    let width_location = app.locate(width).unwrap();
    assert_eq!(width_location.owner_scope, 1);
    assert_eq!(width_location.owner_name, None);
    let dependents = app.dependents(width_location).unwrap();
    assert_eq!(dependents.len(), 1);
    assert_eq!(dependents[0].kind, WatcherKind::Layout);
    assert_eq!(dependents[0].label, Some("panel"));
    assert!(dependents[0].layout_node.is_some());

    let node = app.layout()[0].node;
    let taffy_node = app.layout()[0].taffy_node;
    assert_eq!(app.layout()[0].rect.width, 100.0);
    let before = app.stats();
    assert_eq!(width.try_set(100.0), Ok(false));
    app.flush().unwrap();
    assert_eq!(app.stats().relayout_runs, before.relayout_runs);

    width.set(240.0);
    app.flush().unwrap();
    assert_eq!(app.layout()[0].node, node);
    assert_eq!(app.layout()[0].taffy_node, taffy_node);
    assert_eq!(app.layout()[0].rect.width, 240.0);
    assert_eq!(app.stats().relayout_runs, before.relayout_runs + 1);
    assert_eq!(app.stats().signal_writes, before.signal_writes + 1);
}

#[test]
fn explicit_relayout_reruns_layout_without_signal_writes() {
    let app = Engine::new();
    let calls = std::rc::Rc::new(std::cell::Cell::new(0));
    let observed = calls.clone();
    app.mount(move || {
        View::layout("viewport", move || {
            observed.set(observed.get() + 1);
            Rect {
                width: 800.0,
                height: 600.0,
                ..Rect::default()
            }
        })
    })
    .unwrap();
    assert_eq!(calls.get(), 1);
    let before = app.stats();
    app.relayout().unwrap();
    assert_eq!(calls.get(), 2);
    assert_eq!(app.stats().relayout_runs, before.relayout_runs + 1);
    assert_eq!(app.stats().signal_writes, before.signal_writes);
}

#[test]
fn disposed_layout_is_removed_from_results_and_signal_dependents() {
    let app = Engine::new();
    let width = app.source(50.0);
    app.mount(|| {
        View::layout("temporary", move || Rect {
            width: width.get(),
            ..Rect::default()
        })
    })
    .unwrap();
    let location = app.locate(width).unwrap();
    assert_eq!(app.layout().len(), 1);
    app.unmount();
    assert!(app.layout().is_empty());
    // Root-owned sources outlive the mounted UI. Their location remains valid,
    // while the disposed layout no longer appears among dependents.
    assert_eq!(app.locate(width).unwrap(), location);
    assert!(app.dependents(location).unwrap().is_empty());
}
