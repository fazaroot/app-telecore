//! Automation engine: evaluasi trigger/condition/action + loop guard.

pub mod engine;

pub use engine::{render_template, run_workflows, AutomationGuard};
