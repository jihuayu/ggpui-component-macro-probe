use probe_ui::{Engine, LayoutResult, Rect, Signal, View, WatcherKind};
use std::{cell::RefCell, rc::Rc};
use wasm_bindgen::{JsCast, prelude::*};
use web_sys::{CanvasRenderingContext2d, HtmlCanvasElement, MouseEvent};

const WIDTH: u32 = 960;
const HEIGHT: u32 = 540;

struct WebApp {
    engine: Rc<RefCell<Engine>>,
    canvas: HtmlCanvasElement,
    context: CanvasRenderingContext2d,
    selected: [Signal<bool>; 3],
}

impl WebApp {
    fn render(&self) {
        let engine = self.engine.borrow();
        self.context.set_fill_style_str("#0f172a");
        self.context
            .fill_rect(0.0, 0.0, WIDTH.into(), HEIGHT.into());
        self.context.set_fill_style_str("#e2e8f0");
        self.context.set_font("bold 26px system-ui");
        let _ = self
            .context
            .fill_text("ggpui Signal → Taffy → Canvas", 32.0, 46.0);
        self.context.set_font("15px system-ui");
        let _ = self.context.fill_text(
            "点击卡片：只更新对应 Signal 和布局节点（Canvas 示例会整画布重绘）",
            34.0,
            75.0,
        );

        for (index, layout) in engine.layout().into_iter().enumerate() {
            let active = self.selected[index].get();
            self.draw_card(index, layout, active);
        }

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
        self.context.set_fill_style_str("#94a3b8");
        self.context.set_font("14px ui-monospace, monospace");
        let stats = engine.stats();
        let status = format!(
            "layout nodes: {}   active layout watchers: {}   layout runs: {}   signal writes: {}",
            engine.layout().len(),
            dependent_count,
            stats.relayout_runs,
            stats.signal_writes,
        );
        let _ = self.context.fill_text(&status, 34.0, 500.0);
    }

    fn draw_card(&self, index: usize, layout: LayoutResult, active: bool) {
        let rect = layout.rect;
        self.context
            .set_fill_style_str(if active { "#2563eb" } else { "#1e293b" });
        self.context.fill_rect(
            rect.x.into(),
            rect.y.into(),
            rect.width.into(),
            rect.height.into(),
        );
        self.context.set_line_width(if active { 5.0 } else { 2.0 });
        self.context
            .set_stroke_style_str(if active { "#fbbf24" } else { "#475569" });
        self.context.stroke_rect(
            rect.x.into(),
            rect.y.into(),
            rect.width.into(),
            rect.height.into(),
        );
        self.context.set_fill_style_str("#f8fafc");
        self.context.set_font("bold 20px system-ui");
        let _ = self.context.fill_text(
            &format!("Card {}", index + 1),
            f64::from(rect.x + 24.0),
            f64::from(rect.y + 42.0),
        );
        self.context.set_font("14px system-ui");
        let _ = self.context.fill_text(
            if active { "selected" } else { "click me" },
            f64::from(rect.x + 24.0),
            f64::from(rect.y + 70.0),
        );
        self.context.set_font("12px ui-monospace, monospace");
        let _ = self.context.fill_text(
            &format!("node={} taffy={}", layout.node.0, layout.taffy_node),
            f64::from(rect.x + 24.0),
            f64::from(rect.y + rect.height - 18.0),
        );
    }

    fn click(&self, event: MouseEvent) {
        let scale_x = WIDTH as f32 / self.canvas.client_width().max(1) as f32;
        let scale_y = HEIGHT as f32 / self.canvas.client_height().max(1) as f32;
        let x = event.offset_x() as f32 * scale_x;
        let y = event.offset_y() as f32 * scale_y;
        let layouts = self.engine.borrow().layout();
        let Some((index, _)) = layouts.iter().enumerate().find(|(_, layout)| {
            let rect = layout.rect;
            x >= rect.x && x <= rect.x + rect.width && y >= rect.y && y <= rect.y + rect.height
        }) else {
            return;
        };
        self.selected[index].set(!self.selected[index].get());
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

    let engine = Rc::new(RefCell::new(Engine::new()));
    let selected = [
        engine.borrow().source(false),
        engine.borrow().source(false),
        engine.borrow().source(false),
    ];
    let layout_signals = selected;
    engine
        .borrow_mut()
        .mount(move || {
            View::group(vec![
                View::layout("card-1", move || card_rect(0, layout_signals[0])),
                View::layout("card-2", move || card_rect(1, layout_signals[1])),
                View::layout("card-3", move || card_rect(2, layout_signals[2])),
            ])
        })
        .map_err(|error| JsValue::from_str(&format!("mount failed: {error:?}")))?;

    let app = Rc::new(WebApp {
        engine,
        canvas: canvas.clone(),
        context,
        selected,
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

fn card_rect(index: usize, selected: Signal<bool>) -> Rect {
    let active = selected.get();
    Rect {
        x: 34.0 + index as f32 * 300.0,
        y: 125.0,
        width: 260.0,
        height: if active { 250.0 } else { 220.0 },
    }
}
