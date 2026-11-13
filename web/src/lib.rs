use probe_ui::{Engine, LayoutResult, Rect, Signal, View, WatcherKind};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::Rc,
};
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, MouseEvent, Performance, Window};

const WIDTH: u32 = 1120;
const HEIGHT: u32 = 700;
const FLASH_MS: f64 = 280.0;

struct WebApp {
    engine: Rc<RefCell<Engine>>,
    canvas: HtmlCanvasElement,
    context: CanvasRenderingContext2d,
    window: Window,
    performance: Performance,
    selected: [Signal<bool>; 3],
    zoom: Signal<f32>,
    expanded: Signal<bool>,
    flash_until: Rc<RefCell<BTreeMap<&'static str, f64>>>,
    repaint_scheduled: Rc<Cell<bool>>,
}

impl WebApp {
    fn render(&self) {
        let now = self.performance.now();
        self.flash_until
            .borrow_mut()
            .retain(|_, deadline| *deadline > now);
        let layouts = {
            let engine = self.engine.borrow();
            engine.layout()
        };

        self.context.set_fill_style_str("#070b16");
        self.context
            .fill_rect(0.0, 0.0, WIDTH.into(), HEIGHT.into());
        self.context.set_fill_style_str("#e2e8f0");
        self.context.set_font("bold 27px system-ui");
        let _ =
            self.context
                .fill_text("Signal update → local relayout → repaint flash", 34.0, 42.0);
        self.context.set_fill_style_str("#94a3b8");
        self.context.set_font("14px system-ui");
        let _ = self.context.fill_text(
            "父节点驱动 Flex 布局，子节点按 key 保持身份；点击卡片只让相关更新区域闪烁",
            36.0,
            70.0,
        );

        self.draw_zoom_controls();
        self.draw_flex_tree(&layouts, now);
        self.draw_stats(&layouts);
        if !self.flash_until.borrow().is_empty() {
            self.schedule_repaint();
        }
    }

    fn draw_zoom_controls(&self) {
        let zoom = self.zoom.get();
        self.context.set_fill_style_str("#172033");
        self.context.fill_rect(870.0, 28.0, 214.0, 62.0);
        self.context.set_stroke_style_str("#334155");
        self.context.set_line_width(1.0);
        self.context.stroke_rect(870.0, 28.0, 214.0, 62.0);
        self.context.set_fill_style_str("#e2e8f0");
        self.context.set_font("bold 14px system-ui");
        let _ = self.context.fill_text("Flex zoom", 886.0, 51.0);
        self.context.set_font("15px ui-monospace, monospace");
        let _ = self.context.fill_text(&format!("{zoom:.1}x"), 965.0, 51.0);
        self.context.set_fill_style_str("#2563eb");
        self.context.fill_rect(884.0, 61.0, 74.0, 20.0);
        self.context.fill_rect(995.0, 61.0, 74.0, 20.0);
        self.context.set_fill_style_str("#f8fafc");
        self.context.set_font("bold 16px system-ui");
        let _ = self.context.fill_text("−", 916.0, 77.0);
        let _ = self.context.fill_text("+", 1027.0, 77.0);
    }

    fn draw_flex_tree(&self, layouts: &[LayoutResult], now: f64) {
        let Some(parent) = layouts.iter().find(|layout| layout.label == "flex-parent") else {
            return;
        };
        let parent_flash = self.is_flashing(parent.label, now);
        let parent_rect = parent.rect;
        self.context
            .set_fill_style_str(if parent_flash { "#f59e0b" } else { "#111c33" });
        self.context.fill_rect(
            parent_rect.x.into(),
            parent_rect.y.into(),
            parent_rect.width.into(),
            parent_rect.height.into(),
        );
        self.context
            .set_stroke_style_str(if parent_flash { "#fde68a" } else { "#38bdf8" });
        self.context
            .set_line_width(if parent_flash { 5.0 } else { 2.0 });
        self.context.stroke_rect(
            parent_rect.x.into(),
            parent_rect.y.into(),
            parent_rect.width.into(),
            parent_rect.height.into(),
        );
        self.context.set_fill_style_str("#bae6fd");
        self.context.set_font("bold 18px system-ui");
        let _ = self.context.fill_text(
            "Flex parent  (row / space-between)",
            f64::from(parent_rect.x + 20.0),
            f64::from(parent_rect.y + 32.0),
        );
        self.context.set_font("12px ui-monospace, monospace");
        let _ = self.context.fill_text(
            &format!(
                "parent node={} taffy={}  {}",
                parent.node.0,
                parent.taffy_node,
                if self.expanded.get() {
                    "expanded"
                } else {
                    "compact"
                }
            ),
            f64::from(parent_rect.x + 20.0),
            f64::from(parent_rect.y + 53.0),
        );

        let children: Vec<_> = layouts
            .iter()
            .filter(|layout| layout.label.starts_with("flex-child-"))
            .collect();
        for child in children {
            let index = child
                .label
                .strip_prefix("flex-child-")
                .and_then(|value| value.parse::<usize>().ok())
                .unwrap_or(1)
                .saturating_sub(1);
            let active = self.selected.get(index).is_some_and(|signal| signal.get());
            let flashing = self.is_flashing(child.label, now);
            self.draw_connector(parent_rect, child.rect, flashing);
            self.draw_child(child, active, flashing, index + 1);
        }
    }

    fn draw_connector(&self, parent: Rect, child: Rect, flashing: bool) {
        self.context.begin_path();
        self.context
            .set_stroke_style_str(if flashing { "#fbbf24" } else { "#334155" });
        self.context
            .set_line_width(if flashing { 4.0 } else { 1.0 });
        self.context.move_to(
            f64::from(child.x + child.width / 2.0),
            f64::from(parent.y + parent.height),
        );
        self.context
            .line_to(f64::from(child.x + child.width / 2.0), f64::from(child.y));
        self.context.stroke();
    }

    fn draw_child(&self, layout: &LayoutResult, active: bool, flashing: bool, index: usize) {
        let rect = layout.rect;
        self.context.set_fill_style_str(if flashing {
            "#f97316"
        } else if active {
            "#2563eb"
        } else {
            "#1e293b"
        });
        self.context.fill_rect(
            rect.x.into(),
            rect.y.into(),
            rect.width.into(),
            rect.height.into(),
        );
        self.context
            .set_stroke_style_str(if flashing { "#fde68a" } else { "#475569" });
        self.context
            .set_line_width(if flashing { 5.0 } else { 2.0 });
        self.context.stroke_rect(
            rect.x.into(),
            rect.y.into(),
            rect.width.into(),
            rect.height.into(),
        );
        self.context.set_fill_style_str("#f8fafc");
        self.context.set_font("bold 19px system-ui");
        let _ = self.context.fill_text(
            &format!("Child {index}"),
            f64::from(rect.x + 16.0),
            f64::from(rect.y + 32.0),
        );
        self.context.set_font("14px system-ui");
        let _ = self.context.fill_text(
            if active {
                "selected → parent"
            } else {
                "click to update"
            },
            f64::from(rect.x + 16.0),
            f64::from(rect.y + 59.0),
        );
        self.context.set_font("11px ui-monospace, monospace");
        let _ = self.context.fill_text(
            &format!("node={}  taffy={}", layout.node.0, layout.taffy_node),
            f64::from(rect.x + 16.0),
            f64::from(rect.y + rect.height - 14.0),
        );
    }

    fn draw_stats(&self, layouts: &[LayoutResult]) {
        let engine = self.engine.borrow();
        let mut dependent_count = 0;
        for signal in self.selected {
            if let Ok(location) = engine.locate(signal) {
                dependent_count += engine
                    .dependents(location)
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|dependent| dependent.kind == WatcherKind::Layout)
                    .count();
            }
        }
        if let Ok(location) = engine.locate(self.zoom) {
            dependent_count += engine
                .dependents(location)
                .unwrap_or_default()
                .into_iter()
                .filter(|dependent| dependent.kind == WatcherKind::Layout)
                .count();
        }
        if let Ok(location) = engine.locate(self.expanded) {
            dependent_count += engine
                .dependents(location)
                .unwrap_or_default()
                .into_iter()
                .filter(|dependent| dependent.kind == WatcherKind::Layout)
                .count();
        }
        let stats = engine.stats();
        self.context.set_fill_style_str("#94a3b8");
        self.context.set_font("13px ui-monospace, monospace");
        let _ = self.context.fill_text(
            &format!(
                "layout nodes={}  layout deps={}  relayout runs={}  signal writes={}  flashing={}",
                layouts.len(),
                dependent_count,
                stats.relayout_runs,
                stats.signal_writes,
                self.flash_until.borrow().len()
            ),
            38.0,
            635.0,
        );
        self.context.set_font("13px system-ui");
        let _ = self.context.fill_text(
            "点击父节点可切换 expanded/compact；点击 −/+ 调整 Flex 容器缩放。橙色区域就是本次重绘提示。",
            38.0,
            665.0,
        );
    }

    fn is_flashing(&self, label: &'static str, now: f64) -> bool {
        self.flash_until
            .borrow()
            .get(label)
            .is_some_and(|deadline| *deadline > now)
    }

    fn mark_dependents<T: Clone + PartialEq + 'static>(&self, signal: Signal<T>, now: f64) {
        let engine = self.engine.borrow();
        if let Ok(location) = engine.locate(signal) {
            for dependent in engine.dependents(location).unwrap_or_default() {
                if let Some(label) = dependent.label {
                    self.flash_until.borrow_mut().insert(label, now + FLASH_MS);
                }
            }
        }
    }

    fn schedule_repaint(&self) {
        if self.repaint_scheduled.replace(true) {
            return;
        }
        let callback = Closure::once_into_js({
            let app = self.shared_clone();
            move || {
                app.repaint_scheduled.set(false);
                app.render();
            }
        });
        let _ = self
            .window
            .set_timeout_with_callback_and_timeout_and_arguments_0(callback.unchecked_ref(), 32);
    }

    fn shared_clone(&self) -> Rc<Self> {
        Rc::new(Self {
            engine: self.engine.clone(),
            canvas: self.canvas.clone(),
            context: self.context.clone(),
            window: self.window.clone(),
            performance: self.performance.clone(),
            selected: self.selected,
            zoom: self.zoom,
            expanded: self.expanded,
            flash_until: self.flash_until.clone(),
            repaint_scheduled: self.repaint_scheduled.clone(),
        })
    }

    fn click(self: &Rc<Self>, event: MouseEvent) {
        let scale_x = WIDTH as f32 / self.canvas.client_width().max(1) as f32;
        let scale_y = HEIGHT as f32 / self.canvas.client_height().max(1) as f32;
        let x = event.offset_x() as f32 * scale_x;
        let y = event.offset_y() as f32 * scale_y;
        let now = self.performance.now();

        if (884.0..=958.0).contains(&x) && (61.0..=84.0).contains(&y) {
            let next = (self.zoom.get() - 0.1).max(0.8);
            if self.zoom.try_set(next).unwrap_or(false) {
                self.mark_dependents(self.zoom, now);
                self.flush_and_render();
            }
            return;
        }
        if (995.0..=1069.0).contains(&x) && (61.0..=84.0).contains(&y) {
            let next = (self.zoom.get() + 0.1).min(1.2);
            if self.zoom.try_set(next).unwrap_or(false) {
                self.mark_dependents(self.zoom, now);
                self.flush_and_render();
            }
            return;
        }

        let layouts = self.engine.borrow().layout();
        if let Some(parent) = layouts.iter().find(|layout| layout.label == "flex-parent") {
            let rect = parent.rect;
            if x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + 64.0 {
                if self.expanded.try_set(!self.expanded.get()).unwrap_or(false) {
                    self.mark_dependents(self.expanded, now);
                    self.flush_and_render();
                }
                return;
            }
        }
        let Some((index, child)) = layouts
            .iter()
            .filter(|layout| layout.label.starts_with("flex-child-"))
            .enumerate()
            .find(|(_, layout)| {
                let rect = layout.rect;
                x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
            })
        else {
            return;
        };
        let _ = child;
        if self.selected[index]
            .try_set(!self.selected[index].get())
            .unwrap_or(false)
        {
            self.mark_dependents(self.selected[index], now);
            self.flush_and_render();
        }
    }

    fn flush_and_render(&self) {
        self.engine.borrow().flush().expect("canvas update flush");
        self.render();
    }
}

#[wasm_bindgen(start)]
pub fn start() -> Result<(), JsValue> {
    let window = web_sys::window().ok_or_else(|| JsValue::from_str("missing window"))?;
    let document = window
        .document()
        .ok_or_else(|| JsValue::from_str("missing document"))?;
    let canvas = document
        .create_element("canvas")?
        .dyn_into::<HtmlCanvasElement>()?;
    canvas.set_width(WIDTH);
    canvas.set_height(HEIGHT);
    canvas.set_id("probe-canvas");
    document
        .body()
        .ok_or_else(|| JsValue::from_str("missing body"))?
        .append_child(&canvas)?;
    let context = canvas
        .get_context("2d")?
        .ok_or_else(|| JsValue::from_str("2d context unavailable"))?
        .dyn_into::<CanvasRenderingContext2d>()?;
    let performance = window
        .performance()
        .ok_or_else(|| JsValue::from_str("performance API unavailable"))?;

    let engine = Rc::new(RefCell::new(Engine::new()));
    let selected = [
        engine.borrow().source(false),
        engine.borrow().source(false),
        engine.borrow().source(false),
    ];
    let zoom = engine.borrow().source(1.0_f32);
    let expanded = engine.borrow().source(true);
    let layout_signals = selected;
    let layout_zoom = zoom;
    let layout_expanded = expanded;
    engine
        .borrow_mut()
        .mount(move || {
            View::group(vec![
                View::layout("flex-parent", move || {
                    flex_parent_rect(layout_zoom, layout_expanded, layout_signals)
                }),
                View::layout("flex-child-1", move || {
                    flex_child_rect(0, layout_zoom, layout_expanded, layout_signals[0])
                }),
                View::layout("flex-child-2", move || {
                    flex_child_rect(1, layout_zoom, layout_expanded, layout_signals[1])
                }),
                View::layout("flex-child-3", move || {
                    flex_child_rect(2, layout_zoom, layout_expanded, layout_signals[2])
                }),
            ])
        })
        .map_err(|error| JsValue::from_str(&format!("mount failed: {error:?}")))?;

    let app = Rc::new(WebApp {
        engine,
        canvas: canvas.clone(),
        context,
        window,
        performance,
        selected,
        zoom,
        expanded,
        flash_until: Rc::new(RefCell::new(BTreeMap::new())),
        repaint_scheduled: Rc::new(Cell::new(false)),
    });
    let listener = app.clone();
    let callback = Closure::wrap(
        Box::new(move |event: MouseEvent| listener.click(event)) as Box<dyn FnMut(MouseEvent)>
    );
    canvas.add_event_listener_with_callback("click", callback.as_ref().unchecked_ref())?;
    callback.forget();
    app.render();
    Ok(())
}

fn active_count(selected: [Signal<bool>; 3]) -> usize {
    selected.into_iter().filter(|signal| signal.get()).count()
}

fn flex_parent_rect(
    zoom: Signal<f32>,
    expanded: Signal<bool>,
    selected: [Signal<bool>; 3],
) -> Rect {
    let scale = zoom.get();
    let active = active_count(selected);
    Rect {
        x: 42.0,
        y: 126.0,
        width: 900.0 * scale,
        height: (if expanded.get() { 360.0 } else { 250.0 }) * scale + active as f32 * 5.0,
    }
}

fn flex_child_rect(
    index: usize,
    zoom: Signal<f32>,
    expanded: Signal<bool>,
    selected: Signal<bool>,
) -> Rect {
    let scale = zoom.get();
    let parent = Rect {
        x: 42.0,
        y: 126.0,
        width: 900.0 * scale,
        height: (if expanded.get() { 360.0 } else { 250.0 }) * scale,
    };
    let gap = 18.0 * scale;
    let inner_width = parent.width - 48.0 * scale;
    let child_width = (inner_width - gap * 2.0) / 3.0;
    let active = selected.get();
    Rect {
        x: parent.x + 24.0 * scale + index as f32 * (child_width + gap),
        y: parent.y + 82.0 * scale,
        width: child_width,
        height: if expanded.get() {
            (if active { 205.0 } else { 180.0 }) * scale
        } else {
            92.0 * scale
        },
    }
}
