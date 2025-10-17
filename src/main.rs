use bevy::color::palettes::css;
use bevy::picking::prelude::*;
use bevy::prelude::*;
use bevy::ui::{BackgroundColor, Node, Overflow, OverflowAxis};
use bevy::window::WindowResized;
use bevy_tween::component_tween_system;
use bevy_tween::prelude::*;
use std::time::Duration;

// Tunables
const SLIDE_DURATION_MS: u64 = 200;
const DRAG_COMMIT_THRESHOLD_FRAC: f32 = 0.05; // of view width
const SNAP_HALF_FRAC: f32 = 0.5; // snap to next if passed halfway

// Custom interpolator for Style.left property
struct StyleLeftInterpolator {
    start: f32,
    end: f32,
}

impl Interpolator for StyleLeftInterpolator {
    type Item = Node;

    fn interpolate(&self, item: &mut Self::Item, value: f32, _previous_value: f32) {
        item.left = Val::Px(self.start + (self.end - self.start) * value);
    }
}

fn style_left(start: f32, end: f32) -> StyleLeftInterpolator {
    StyleLeftInterpolator { start, end }
}

#[derive(Component)]
struct PageTrack;

#[derive(Component)]
struct Page(#[allow(dead_code)] pub usize);

#[derive(Component, Debug)]
struct Slider {
    page_count: usize,
    current: usize,
    view_w: f32,
    pending_steps: i32,
    post_action: PostAction,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
enum PostAction {
    #[default]
    None,
    RotateFirstToEndResetToZero,
}

#[derive(Component, Debug, Clone, Copy)]
struct MouseDrag {
    start: Vec2,
    start_left: f32,
}

#[derive(Component)]
struct SlideTweenTimer(Timer);

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(DefaultTweenPlugins)
        .add_tween_systems(component_tween_system::<StyleLeftInterpolator>())
        .insert_resource(UiPickingSettings {
            require_markers: true,
        })
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                keyboard_nav,
                process_pending_steps,
                tick_slide_tween,
                handle_window_resize,
            )
                .chain(),
        )
        .run();
}

fn setup(mut commands: Commands, windows: Query<&Window>) {
    commands.spawn((Camera2d, UiPickingCamera));
    let Ok(window) = windows.single() else {
        return;
    };
    let view_w = window.resolution.width();
    let page_count = 3usize;

    // Root
    let root = commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            ..default()
        })
        .id();

    // Viewport (container with overflow clip)
    let viewport = commands
        .spawn((
            Node {
                width: Val::Percent(100.0),
                height: Val::Percent(100.0),
                overflow: Overflow {
                    x: OverflowAxis::Clip,
                    y: OverflowAxis::Clip,
                },
                ..default()
            },
            BackgroundColor(Color::srgba(0.05, 0.06, 0.08, 1.0)),
        ))
        .id();

    // Track (no duplicates)
    let track = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                left: Val::Px(0.0),
                top: Val::Px(0.0),
                width: Val::Px(page_count as f32 * view_w),
                height: Val::Percent(100.0),
                flex_direction: FlexDirection::Row,
                ..default()
            },
            BackgroundColor(Color::NONE),
            PageTrack,
            Pickable::default(),
            Slider {
                page_count,
                current: 0,
                view_w,
                pending_steps: 0,
                post_action: PostAction::None,
            },
        ))
        .observe(on_track_drag_start)
        .observe(on_track_drag)
        .observe(on_track_drag_end)
        .observe(on_track_drag_cancel)
        .id();

    commands.entity(root).add_child(viewport);
    commands.entity(viewport).add_child(track);

    let colors = [css::RED, css::BLUE, css::GREEN];

    for i in 0..page_count {
        commands.entity(track).with_children(|p| {
            p.spawn((
                Node {
                    width: Val::Px(view_w),
                    height: Val::Percent(100.0),
                    justify_content: JustifyContent::Center,
                    align_items: AlignItems::Center,
                    ..default()
                },
                BackgroundColor(colors[i % colors.len()].into()),
                Page(i),
            ));
        });
    }
}

fn handle_window_resize(
    mut resize_events: EventReader<WindowResized>,
    mut slider: Query<(&mut Slider, &mut Node), With<PageTrack>>,
) {
    for event in resize_events.read() {
        if let Ok((mut slider, mut node)) = slider.single_mut() {
            let new_width = event.width;
            slider.view_w = new_width;
            node.width = Val::Px(slider.page_count as f32 * new_width);
            node.left = Val::Px(-(slider.current as f32) * new_width);
        }
    }
}

fn keyboard_nav(
    keys: Res<ButtonInput<KeyCode>>,
    mut q: Query<(&mut Slider, Has<SlideTweenTimer>), With<PageTrack>>,
) {
    let Ok((mut slider, _animating)) = q.single_mut() else {
        return;
    };

    // Always queue steps, even during animation
    let mut delta = 0i32;
    if keys.just_pressed(KeyCode::ArrowRight) || keys.just_pressed(KeyCode::KeyD) {
        delta += 1;
    }
    if keys.just_pressed(KeyCode::ArrowLeft) || keys.just_pressed(KeyCode::KeyA) {
        delta -= 1;
    }
    if delta != 0 {
        slider.pending_steps += delta;
    }
}

// Observer: start drag on track (DragStart)
fn on_track_drag_start(
    trigger: Trigger<Pointer<DragStart>>,
    mut slider: Query<&mut Slider>,
    node: Query<&Node>,
    mut commands: Commands,
    animating_q: Query<(), With<SlideTweenTimer>>,
) {
    let track_e = trigger.target();
    // If an animation is running, ignore drag start
    if animating_q.get(track_e).is_ok() {
        return;
    }

    let Ok(track_node) = node.get(track_e) else {
        return;
    };
    let Ok(mut slider) = slider.get_mut(track_e) else {
        return;
    };

    slider.post_action = PostAction::None;

    commands.entity(track_e).insert(MouseDrag {
        start: trigger.event().pointer_location.position,
        start_left: get_left_px(track_node),
    });
}

// Observer: dragging over the track (Drag)
fn on_track_drag(
    trigger: Trigger<Pointer<Drag>>,
    mut q: Query<(
        &Children,
        &mut Node,
        &mut Slider,
        Has<SlideTweenTimer>,
        Option<&mut MouseDrag>,
    )>,
    mut commands: Commands,
) {
    let track_e = trigger.target();
    let Ok((children, mut node, mut slider, animating, mouse_drag)) = q.get_mut(track_e) else {
        return;
    };

    if animating {
        return;
    }

    if let Some(mut md) = mouse_drag {
        let current = trigger.event().pointer_location.position;
        let dx = current.x - md.start.x;
        let view_width = slider.view_w;
        let mut left = md.start_left + dx;

        while left > 0.0 {
            tracing::info!(
                "on_track_drag: rotating LAST to front, left={left:.1}, current={} -> {}",
                slider.current,
                (slider.current + slider.page_count - 1) % slider.page_count
            );
            rotate_last_to_front(track_e, children, &mut commands);
            left -= view_width;
            slider.current = (slider.current + slider.page_count - 1) % slider.page_count;
            md.start_left -= view_width;
        }
        while left < -view_width {
            tracing::info!(
                "on_track_drag: rotating FIRST to end, left={left:.1}, current={} -> {}",
                slider.current,
                (slider.current + 1) % slider.page_count
            );
            rotate_first_to_end(track_e, children, &mut commands);
            left += view_width;
            slider.current = (slider.current + 1) % slider.page_count;
            md.start_left += view_width;
        }

        node.left = Val::Px(left);
    }
}

// Observer: end drag on track (DragEnd)
fn on_track_drag_end(
    trigger: Trigger<Pointer<DragEnd>>,
    mut track_q: Query<(&Children, &mut Node, &mut Slider, Option<&MouseDrag>)>,
    mut commands: Commands,
) {
    let track_e = trigger.target();
    handle_drag_finish_like(
        track_e,
        &mut track_q,
        &mut commands,
        Some(trigger.event().pointer_location.position),
    );
}

fn process_pending_steps(
    mut q: Query<
        (
            Entity,
            &Children,
            &mut Node,
            &mut Slider,
            Has<SlideTweenTimer>,
            Option<&MouseDrag>,
        ),
        With<PageTrack>,
    >,
    mut commands: Commands,
) {
    let Ok((entity, children, mut node, mut slider, animating, mouse_drag)) = q.single_mut() else {
        return;
    };

    if animating || mouse_drag.is_some() || slider.pending_steps == 0 {
        return;
    }

    let view_width = slider.view_w;

    if slider.pending_steps > 0 {
        slider.pending_steps -= 1;
        start_next_tween(entity, &mut node, &mut slider, view_width, &mut commands);
    } else {
        slider.pending_steps += 1;
        rotate_last_to_front(entity, children, &mut commands);
        slider.current = (slider.current + slider.page_count - 1) % slider.page_count;
        node.left = Val::Px(-view_width);
        start_back_tween(entity, &mut node, &mut slider, &mut commands);
    }
}

// Observer: drag canceled (treat like end with no extra delta)
fn on_track_drag_cancel(
    trigger: Trigger<Pointer<Cancel>>,
    mut track_q: Query<(&Children, &mut Node, &mut Slider, Option<&MouseDrag>)>,
    mut commands: Commands,
) {
    let track_e = trigger.target();
    handle_drag_finish_like(track_e, &mut track_q, &mut commands, None);
}

// Shared logic for ending/canceling a drag
fn handle_drag_finish_like(
    track_e: Entity,
    track_q: &mut Query<(&Children, &mut Node, &mut Slider, Option<&MouseDrag>)>,
    commands: &mut Commands,
    end_pos_opt: Option<Vec2>,
) {
    if let Ok((children, mut node, mut slider, Some(md))) = track_q.get_mut(track_e) {
        let left = get_left_px(&node);
        let mut left_norm = left;
        if left_norm > 0.0 {
            left_norm -= slider.view_w;
        }
        let view_w = slider.view_w;
        let dx_total = if let Some(end_pos) = end_pos_opt {
            end_pos.x - md.start.x
        } else {
            // Cancel: treat as zero movement to snap to nearest
            0.0
        };
        let threshold_px = view_w * DRAG_COMMIT_THRESHOLD_FRAC;

        if dx_total <= -threshold_px {
            start_next_tween(track_e, &mut node, &mut slider, view_w, commands);
        } else if dx_total >= threshold_px {
            if get_left_px(&node) >= 0.0 {
                tracing::info!(
                    "handle_drag_finish: EXTRA rotation needed, left={:.1}, current={} -> {}",
                    get_left_px(&node),
                    slider.current,
                    (slider.current + slider.page_count - 1) % slider.page_count
                );
                rotate_last_to_front(track_e, children, commands);
                slider.current = (slider.current + slider.page_count - 1) % slider.page_count;
                node.left = Val::Px(-view_w);
            }
            tracing::info!("handle_drag_finish: calling start_back_tween, dx_total={dx_total:.1}, left={:.1}, current={}", get_left_px(&node), slider.current);
            start_back_tween(track_e, &mut node, &mut slider, commands);
        } else if -left_norm >= view_w * SNAP_HALF_FRAC {
            start_next_tween(track_e, &mut node, &mut slider, view_w, commands);
        } else {
            start_back_tween(track_e, &mut node, &mut slider, commands);
        }
        commands.entity(track_e).remove::<MouseDrag>();
    }
}

// Helpers

fn get_left_px(node: &Node) -> f32 {
    match node.left {
        Val::Px(v) => v,
        _ => 0.0,
    }
}

fn start_next_tween(
    track: Entity,
    node: &mut Node,
    slider: &mut Slider,
    view_width: f32,
    commands: &mut Commands,
) {
    let start = get_left_px(node);
    let end = -view_width;
    slider.post_action = PostAction::RotateFirstToEndResetToZero;

    // Cancel any existing animation by removing the timer component
    // This prevents old animations' post_actions from executing
    commands.entity(track).remove::<SlideTweenTimer>();

    let target = track.into_target();
    commands.entity(track).animation().insert_tween_here(
        Duration::from_millis(SLIDE_DURATION_MS),
        EaseKind::CubicOut,
        target.with(style_left(start, end)),
    );
    // mark animating
    commands
        .entity(track)
        .insert(SlideTweenTimer(Timer::from_seconds(
            SLIDE_DURATION_MS as f32 / 1000.0,
            TimerMode::Once,
        )));
}

fn start_back_tween(track: Entity, node: &mut Node, slider: &mut Slider, commands: &mut Commands) {
    let start = get_left_px(node);
    let end = 0.0;
    slider.post_action = PostAction::None;
    tracing::info!(
        "start_back_tween: start={start:.1}, end={end}, current={}",
        slider.current
    );

    // Cancel any existing animation by removing the timer component
    // This prevents old animations' post_actions from executing
    commands.entity(track).remove::<SlideTweenTimer>();

    let target = track.into_target();
    commands.entity(track).animation().insert_tween_here(
        Duration::from_millis(SLIDE_DURATION_MS),
        EaseKind::CubicOut,
        target.with(style_left(start, end)),
    );
    // mark animating
    commands
        .entity(track)
        .insert(SlideTweenTimer(Timer::from_seconds(
            SLIDE_DURATION_MS as f32 / 1000.0,
            TimerMode::Once,
        )));
}

fn tick_slide_tween(
    time: Res<Time>,
    mut q: Query<
        (
            Entity,
            &Children,
            &mut Node,
            &mut Slider,
            &mut SlideTweenTimer,
        ),
        With<PageTrack>,
    >,
    mut commands: Commands,
) {
    let Ok((track_e, children, mut node, mut slider, mut timer)) = q.single_mut() else {
        return;
    };

    timer.0.tick(time.delta());
    if timer.0.finished() {
        let left_px = get_left_px(&node);
        tracing::info!(
            "tick_slide_tween COMPLETE: left_px={left_px:.1}, post_action={:?}, current={}",
            slider.post_action,
            slider.current
        );
        // complete any pending post action
        if slider.post_action == PostAction::RotateFirstToEndResetToZero {
            rotate_first_to_end(track_e, children, &mut commands);
            slider.current = (slider.current + 1) % slider.page_count;
            node.left = Val::Px(0.0);
            tracing::info!(
                "tick_slide_tween ROTATED forward, current now {}",
                slider.current
            );
        }
        slider.post_action = PostAction::None;
        commands.entity(track_e).remove::<SlideTweenTimer>();
    }
}

fn rotate_first_to_end(track: Entity, children: &Children, commands: &mut Commands) {
    if children.is_empty() {
        return;
    }
    let first = children[0];
    commands.entity(track).remove_children(&[first]);
    commands.entity(track).add_child(first);
}

fn rotate_last_to_front(track: Entity, children: &Children, commands: &mut Commands) {
    if children.is_empty() {
        return;
    }
    let last = *children.last().unwrap();
    commands.entity(track).remove_children(&[last]);
    commands.entity(track).insert_children(0, &[last]);
}
