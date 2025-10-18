use bevy::color::palettes::css;
use bevy::math::curve::easing::EaseFunction;
use bevy::picking::prelude::*;
use bevy::prelude::*;
use bevy::ui::{BackgroundColor, Node, Overflow, OverflowAxis, ZIndex};
use bevy::window::WindowResized;

// Tunables
const SLIDE_DURATION_MS: u64 = 200;
const SLIDE_EASE: EaseFunction = EaseFunction::CubicOut;
const DRAG_COMMIT_THRESHOLD_FRAC: f32 = 0.05; // of view width
const SNAP_HALF_FRAC: f32 = 0.5; // snap to next if passed halfway

// Simple custom slide animation to avoid scheduling/event race issues.
#[derive(Component, Debug)]
struct SlideAnim {
    start: f32,
    end: f32,
    elapsed: f32,
    duration: f32,
    ease: EaseFunction,
}

impl SlideAnim {
    fn new(start: f32, end: f32, duration_ms: u64, ease: EaseFunction) -> Self {
        Self {
            start,
            end,
            elapsed: 0.0,
            duration: duration_ms as f32 / 1000.0,
            ease,
        }
    }
}

fn ease_cubic_out(t: f32) -> f32 {
    let n = 1.0 - t;
    1.0 - n * n * n
}

// Lens to tween Node.left (Val::Px) between two pixel values.
#[derive(Debug, Clone, Copy)]
struct NodeLeftLens {
    start: f32,
    end: f32,
}
impl NodeLeftLens {
    fn sample(&self, ratio: f32) -> f32 {
        let t = ratio.clamp(0.0, 1.0);
        self.start + (self.end - self.start) * t
    }
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

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .insert_resource(UiPickingSettings {
            require_markers: true,
        })
        .add_systems(Startup, setup)
        .add_systems(
            Update,
            (
                keyboard_nav,
                process_pending_steps,
                tick_slide_anim,
                handle_window_resize,
            )
                .chain(),
        )
        .run();
}

fn setup(mut commands: Commands, windows: Query<&Window>) {
    commands.spawn((Camera2d, Camera::default(), Msaa::Sample4, UiPickingCamera));
    let Ok(window) = windows.single() else {
        return;
    };
    let view_w = window.resolution.width() / window.resolution.scale_factor();
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

    // Bottom nav bar
    let nav_bar = commands
        .spawn((
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(16.0),
                left: Val::Percent(0.0),
                width: Val::Percent(100.0),
                height: Val::Px(60.0),
                justify_content: JustifyContent::Center,
                align_items: AlignItems::Center,
                column_gap: Val::Px(16.0),
                ..default()
            },
            BackgroundColor(Color::NONE),
            ZIndex(1),
        ))
        .id();

    let btn_node = Node {
        width: Val::Px(140.0),
        height: Val::Px(44.0),
        justify_content: JustifyContent::Center,
        align_items: AlignItems::Center,
        ..default()
    };

    commands.entity(root).add_child(nav_bar);
    commands.entity(nav_bar).with_children(|p| {
        p.spawn((
            Button,
            btn_node.clone(),
            BackgroundColor(Color::srgba(0.2, 0.2, 0.25, 0.9)),
            Pickable::default(),
        ))
        .observe(on_prev_click);
        p.spawn((
            Button,
            btn_node,
            BackgroundColor(Color::srgba(0.2, 0.2, 0.25, 0.9)),
            Pickable::default(),
        ))
        .observe(on_next_click);
    });
}

// Observer: previous button clicked
fn on_prev_click(
    _trigger: On<Pointer<Click>>,
    mut track_q: Query<(&mut Slider, Option<&SlideAnim>), With<PageTrack>>,
) {
    let Ok((mut slider, _anim)) = track_q.single_mut() else {
        return;
    };
    slider.pending_steps -= 1;
}

// Observer: next button clicked
fn on_next_click(
    _trigger: On<Pointer<Click>>,
    mut track_q: Query<(&mut Slider, Option<&SlideAnim>), With<PageTrack>>,
) {
    let Ok((mut slider, _anim)) = track_q.single_mut() else {
        return;
    };
    slider.pending_steps += 1;
}

fn keyboard_nav(
    keys: Res<ButtonInput<KeyCode>>,
    mut q: Query<(&mut Slider, Option<&SlideAnim>), With<PageTrack>>,
) {
    let Ok((mut slider, _animator)) = q.single_mut() else {
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
    trigger: On<Pointer<DragStart>>,
    mut slider: Query<&mut Slider>,
    node: Query<&Node>,
    mut commands: Commands,
) {
    let track_e = trigger.entity;
    let Ok(track_node) = node.get(track_e) else {
        return;
    };
    let Ok(mut slider) = slider.get_mut(track_e) else {
        return;
    };

    commands.entity(track_e).remove::<SlideAnim>();
    slider.post_action = PostAction::None;

    commands.entity(track_e).insert(MouseDrag {
        start: trigger.event().pointer_location.position,
        start_left: get_left_px(track_node),
    });
}

// Observer: dragging over the track (Drag)
fn on_track_drag(
    trigger: On<Pointer<Drag>>,
    mut q: Query<(
        &Children,
        &mut Node,
        &mut Slider,
        Option<&SlideAnim>,
        Option<&mut MouseDrag>,
    )>,
    mut commands: Commands,
) {
    let track_e = trigger.entity;
    let Ok((children, mut node, mut slider, animator, mouse_drag)) = q.get_mut(track_e) else {
        return;
    };

    if animator.is_some() {
        return;
    }

    if let Some(mut md) = mouse_drag {
        let current = trigger.event().pointer_location.position;
        let dx = current.x - md.start.x;
        let view_width = slider.view_w;
        let mut left = md.start_left + dx;

        while left > 0.0 {
            rotate_last_to_front(track_e, children, &mut commands);
            left -= view_width;
            slider.current = (slider.current + slider.page_count - 1) % slider.page_count;
            md.start_left -= view_width;
        }
        while left < -view_width {
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
    trigger: On<Pointer<DragEnd>>,
    mut track_q: Query<(&Children, &mut Node, &mut Slider, Option<&MouseDrag>)>,
    mut commands: Commands,
) {
    let track_e = trigger.entity;
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
            Option<&SlideAnim>,
            Option<&MouseDrag>,
        ),
        With<PageTrack>,
    >,
    mut commands: Commands,
) {
    let Ok((entity, children, mut node, mut slider, animator, mouse_drag)) = q.single_mut() else {
        return;
    };

    if animator.is_some() || mouse_drag.is_some() || slider.pending_steps == 0 {
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
    trigger: On<Pointer<Cancel>>,
    mut track_q: Query<(&Children, &mut Node, &mut Slider, Option<&MouseDrag>)>,
    mut commands: Commands,
) {
    let track_e = trigger.entity;
    handle_drag_finish_like(track_e, &mut track_q, &mut commands, None);
}

// Handle slide animation progression and completion.
fn tick_slide_anim(
    time: Res<Time>,
    mut q: Query<(Entity, &Children, &mut Node, &mut Slider, &mut SlideAnim), With<PageTrack>>,
    mut commands: Commands,
) {
    let Ok((entity, children, mut node, mut slider, mut anim)) = q.single_mut() else {
        return;
    };

    anim.elapsed += time.delta_secs();
    let ratio = (anim.elapsed / anim.duration).clamp(0.0, 1.0);
    let eased = match anim.ease {
        EaseFunction::CubicOut => ease_cubic_out(ratio),
        _ => ratio,
    };

    let lens = NodeLeftLens {
        start: anim.start,
        end: anim.end,
    };
    node.left = Val::Px(lens.sample(eased));

    if ratio >= 1.0 {
        // Finish
        node.left = Val::Px(anim.end);
        commands.entity(entity).remove::<SlideAnim>();
        if slider.post_action == PostAction::RotateFirstToEndResetToZero {
            rotate_first_to_end(entity, children, &mut commands);
            slider.current = (slider.current + 1) % slider.page_count;
            node.left = Val::Px(0.0);
        }
        slider.post_action = PostAction::None;
    }
}

fn handle_window_resize(
    mut eview_width: MessageReader<WindowResized>,
    mut q: Query<
        (
            Entity,
            &Children,
            &mut Node,
            &mut Slider,
            Option<&SlideAnim>,
        ),
        With<PageTrack>,
    >,
    mut q_page: Query<&mut Node, (With<Page>, Without<PageTrack>)>,
    mut commands: Commands,
) {
    let mut new_width: Option<f32> = None;
    for ev in eview_width.read() {
        new_width = Some(ev.width);
    }
    if let Some(view_w) = new_width {
        let Ok((entity, children, mut track_node, mut slider, anim)) = q.single_mut() else {
            return;
        };

        let old_view_w = slider.view_w;
        slider.view_w = view_w;
        track_node.width = Val::Px(slider.page_count as f32 * view_w);

        for child in children {
            if let Ok(mut n) = q_page.get_mut(*child) {
                n.width = Val::Px(view_w);
            }
        }

        if anim.is_some() {
            commands.entity(entity).remove::<SlideAnim>();
        }

        let current_left = get_left_px(&track_node);
        let normalized_pos = current_left / old_view_w;
        let new_left = normalized_pos * view_w;
        track_node.left = Val::Px(new_left);

        slider.post_action = PostAction::None;
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

    commands
        .entity(track)
        .insert(SlideAnim::new(start, end, SLIDE_DURATION_MS, SLIDE_EASE));
}

fn start_back_tween(track: Entity, node: &mut Node, slider: &mut Slider, commands: &mut Commands) {
    let start = get_left_px(node);
    let end = 0.0;
    slider.post_action = PostAction::None;

    commands
        .entity(track)
        .insert(SlideAnim::new(start, end, SLIDE_DURATION_MS, SLIDE_EASE));
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
                rotate_last_to_front(track_e, children, commands);
                slider.current = (slider.current + slider.page_count - 1) % slider.page_count;
                node.left = Val::Px(-view_w);
            }
            start_back_tween(track_e, &mut node, &mut slider, commands);
        } else if -left_norm >= view_w * SNAP_HALF_FRAC {
            start_next_tween(track_e, &mut node, &mut slider, view_w, commands);
        } else {
            start_back_tween(track_e, &mut node, &mut slider, commands);
        }
        commands.entity(track_e).remove::<MouseDrag>();
    }
}
