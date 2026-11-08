//! Pages of apps that follow your fingers. A two-finger swipe moves the pages as you
//! swipe; letting go glides to the nearest page, or on to the next one if you swiped
//! far enough. X11 touchpads never say when a swipe ends, so "letting go" is a short
//! pause in movement. A mouse wheel flips one page per notch.

use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::time::Duration;

use adw::prelude::*;
use gtk::{gdk, glib};

/// Pause in finger movement that counts as letting go.
const SNAP_AFTER: Duration = Duration::from_millis(110);
/// How far (as a share of a page) a swipe must travel to turn the page.
const TURN_AT: f64 = 0.12;
/// Touchpad scroll units to pixels of page movement.
const PIXELS_PER_UNIT: f64 = 45.0;

pub struct Pager {
    pub widget: gtk::Box,
    scroller: gtk::ScrolledWindow,
    row: gtk::Box,
    dots: gtk::Box,
    count: Cell<i32>,
    width: Cell<i32>,
    swipe_from: Cell<Option<f64>>,
    snap: RefCell<Option<glib::SourceId>>,
    glide: RefCell<Option<adw::TimedAnimation>>,
    wheel_at: Cell<i64>,
}

impl Pager {
    pub fn new() -> Rc<Pager> {
        let row = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        let scroller = gtk::ScrolledWindow::builder()
            .hscrollbar_policy(gtk::PolicyType::External)
            .vscrollbar_policy(gtk::PolicyType::Never)
            .kinetic_scrolling(false)
            .vexpand(true)
            .child(&row)
            .build();
        let dots = gtk::Box::new(gtk::Orientation::Horizontal, 8);
        dots.set_halign(gtk::Align::Center);
        dots.set_margin_bottom(12);
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 0);
        widget.append(&scroller);
        widget.append(&dots);

        let pager = Rc::new(Pager {
            widget,
            scroller,
            row,
            dots,
            count: Cell::new(0),
            width: Cell::new(1920),
            swipe_from: Cell::new(None),
            snap: RefCell::default(),
            glide: RefCell::default(),
            wheel_at: Cell::new(0),
        });

        // take every scroll before the scrolled window does, so we decide how pages move
        let scroll = gtk::EventControllerScroll::new(gtk::EventControllerScrollFlags::BOTH_AXES);
        scroll.set_propagation_phase(gtk::PropagationPhase::Capture);
        let weak = Rc::downgrade(&pager);
        scroll.connect_scroll(move |c, dx, dy| match weak.upgrade() {
            Some(p) => p.on_scroll(c, dx, dy),
            None => glib::Propagation::Proceed,
        });
        pager.scroller.add_controller(scroll);

        let weak = Rc::downgrade(&pager);
        pager.scroller.hadjustment().connect_value_changed(move |_| {
            if let Some(p) = weak.upgrade() {
                p.update_dots();
            }
        });
        pager
    }

    /// Replace the pages; each one is a full screen wide.
    pub fn set_pages(self: &Rc<Self>, pages: Vec<gtk::Widget>, width: i32) {
        while let Some(child) = self.row.first_child() {
            self.row.remove(&child);
        }
        while let Some(child) = self.dots.first_child() {
            self.dots.remove(&child);
        }
        self.width.set(width.max(1));
        self.count.set(pages.len() as i32);
        for (i, page) in pages.into_iter().enumerate() {
            let holder = gtk::Box::new(gtk::Orientation::Vertical, 0);
            holder.set_size_request(width, -1);
            page.set_halign(gtk::Align::Center);
            page.set_valign(gtk::Align::Center);
            page.set_vexpand(true);
            holder.append(&page);
            self.row.append(&holder);

            let dot = gtk::Button::builder().css_classes(["pager-dot"]).tooltip_text(format!("Page {}", i + 1)).build();
            let weak = Rc::downgrade(self);
            dot.connect_clicked(move |_| {
                if let Some(p) = weak.upgrade() {
                    p.go_to(i as i32, true);
                }
            });
            self.dots.append(&dot);
        }
        self.dots.set_visible(self.count.get() > 1);
        self.update_dots();
    }

    pub fn current(&self) -> i32 {
        (self.scroller.hadjustment().value() / self.width.get() as f64).round() as i32
    }

    pub fn go_to(&self, page: i32, animate: bool) {
        let page = page.clamp(0, (self.count.get() - 1).max(0));
        let adj = self.scroller.hadjustment();
        let to = page as f64 * self.width.get() as f64;
        self.stop_glide();
        if !animate || (adj.value() - to).abs() < 1.0 {
            adj.set_value(to);
            return;
        }
        let a = adj.clone();
        let target = adw::CallbackAnimationTarget::new(move |v| a.set_value(v));
        let glide = adw::TimedAnimation::new(&self.scroller, adj.value(), to, 280, target);
        glide.set_easing(adw::Easing::EaseOutCubic);
        glide.play();
        self.glide.replace(Some(glide));
    }

    fn stop_glide(&self) {
        if let Some(g) = self.glide.take() {
            g.pause();
        }
    }

    fn on_scroll(self: &Rc<Self>, c: &gtk::EventControllerScroll, dx: f64, dy: f64) -> glib::Propagation {
        let delta = if dx.abs() >= dy.abs() { dx } else { dy };
        let touchpad = c
            .current_event_device()
            .map(|d| d.source() == gdk::InputSource::Touchpad)
            .unwrap_or(false)
            || delta.fract() != 0.0;

        if !touchpad {
            // a mouse wheel: one page per notch
            let now = glib::monotonic_time();
            if delta != 0.0 && now - self.wheel_at.get() > 250_000 {
                self.wheel_at.set(now);
                self.go_to(self.current() + delta.signum() as i32, true);
            }
            return glib::Propagation::Stop;
        }

        // a touchpad: the pages follow your fingers
        self.stop_glide();
        let adj = self.scroller.hadjustment();
        if self.swipe_from.get().is_none() {
            self.swipe_from.set(Some(adj.value()));
        }
        let max = (adj.upper() - adj.page_size()).max(0.0);
        adj.set_value((adj.value() + delta * PIXELS_PER_UNIT).clamp(0.0, max));

        if let Some(id) = self.snap.take() {
            id.remove();
        }
        let weak = Rc::downgrade(self);
        let id = glib::timeout_add_local_once(SNAP_AFTER, move || {
            if let Some(p) = weak.upgrade() {
                p.snap.take();
                p.settle();
            }
        });
        self.snap.replace(Some(id));
        glib::Propagation::Stop
    }

    /// Fingers lifted: turn the page if the swipe went far enough, otherwise spring back.
    fn settle(&self) {
        let value = self.scroller.hadjustment().value();
        let from = self.swipe_from.take().unwrap_or(value);
        let w = self.width.get() as f64;
        let start = (from / w).round() as i32;
        let moved = value - from;
        let target = if moved > TURN_AT * w {
            start + 1
        } else if moved < -TURN_AT * w {
            start - 1
        } else {
            start
        };
        self.go_to(target, true);
    }

    fn update_dots(&self) {
        let current = self.current();
        let mut child = self.dots.first_child();
        let mut i = 0;
        while let Some(dot) = child {
            if i == current {
                dot.add_css_class("active");
            } else {
                dot.remove_css_class("active");
            }
            child = dot.next_sibling();
            i += 1;
        }
    }
}
