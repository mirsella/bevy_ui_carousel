use bevy::color::palettes::css;
use bevy::input::touch::{TouchInput, TouchPhase};
use bevy::prelude::*;
use bevy::ui::{BackgroundColor, Node, Overflow, OverflowAxis};
use bevy::window::WindowResized;
use bevy::math::curve::easing::EaseFunction;
use bevy_tweening::{
    TweenAnim, Tween, AnimCompletedEvent, TweeningPlugin,
    lens::Lens,
};

#[derive(Component)]
struct Viewport;

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

#[derive(Resource, Default)]
struct DragState {
    touch: Option<TouchDrag>,
    mouse: Option<MouseDrag>,
}

#[derive(Debug, Clone, Copy)]
struct TouchDrag {
    id: u64,
    start: Vec2,
    start_left: f32,
}

#[derive(Debug, Clone, Copy)]
struct MouseDrag {
    start: Vec2,
    start_left: f32,
}

#[derive(Component)]
struct NavPrevBtn;

#[derive(Component)]
struct NavNextBtn;

// Lens to tween Node.left (Val::Px) between two pixel values.
#[derive(Debug, Clone, Copy)]
struct NodeLeftLens {
    start: f32,
    end: f32,
}
impl Lens<Node> for NodeLeftLens {
    fn lerp(&mut self, mut target: Mut<'_, Node>, ratio: f32) {
        let t = ratio.clamp(0.0, 1.0);
        let x = self.start + (self.end - self.start) * t;
        target.left = Val::Px(x);
    }
}

fn main() {
    App::new()
        .add_plugins(DefaultPlugins)
        .add_plugins(TweeningPlugin)
        .init_resource::<DragState>()
        .add_systems(Startup, setup)
        .add_systems(
        Update,
        (
            nav_buttons,
            keyboard_nav,
            drag_nav,
            process_pending_steps,
            on_tween_completed,
            handle_window_resize,
        ),
    )
        .run();
}

fn setup(mut commands: Commands, windows: Query<&Window>) {
    commands.spawn(Camera2d);
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

    // Viewport
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
            Viewport,
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
            Slider {
                page_count,
                current: 0,
                view_w,
                pending_steps: 0,
                post_action: PostAction::None,
            },
        ))
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
            NavPrevBtn,
        ));
        p.spawn((
            Button,
            btn_node,
            BackgroundColor(Color::srgba(0.2, 0.2, 0.25, 0.9)),
            NavNextBtn,
        ));
    });
}

fn nav_buttons(
    q_prev: Query<&Interaction, (Changed<Interaction>, With<Button>, With<NavPrevBtn>)>,
    q_next: Query<&Interaction, (Changed<Interaction>, With<Button>, With<NavNextBtn>)>,
    mut q_track: Query<(&mut Slider, Option<&TweenAnim>), With<PageTrack>>,
) {
    let Ok((mut slider, animator)) = q_track.single_mut() else {
        return;
    };

    if animator.is_some() {
        return;
    }

    if let Ok(inter) = q_prev.single() && *inter == Interaction::Pressed {
        slider.pending_steps -= 1;
    }
    if let Ok(inter) = q_next.single() && *inter == Interaction::Pressed {
        slider.pending_steps += 1;
    }
}

fn keyboard_nav(keys: Res<ButtonInput<KeyCode>>, mut q: Query<(&mut Slider, Option<&TweenAnim>), With<PageTrack>>) {
    let Ok((mut slider, animator)) = q.single_mut() else {
        return;
    };
    
    if animator.is_some() {
        return;
    }
    
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

fn drag_nav(
    mut ev_touches: MessageReader<TouchInput>,
    mut ev_cursor: MessageReader<CursorMoved>,
    mouse_buttons: Res<ButtonInput<MouseButton>>,
    mut drag: ResMut<DragState>,
    mut q: Query<
        (
            Entity,
            &Children,
            &mut Node,
            &mut Slider,
            Option<&TweenAnim>,
        ),
        With<PageTrack>,
    >,
    mut commands: Commands,
    windows: Query<&Window>,
) {
    let Ok((track_e, children, mut node, mut slider, animator)) = q.single_mut() else {
        return;
    };
    if animator.is_some() {
        return; // ignore drag during tween
    }

    let view_width = slider.view_w;


    // touch
    for ev in ev_touches.read() {
        match ev.phase {
            TouchPhase::Started => {
                drag.touch = Some(TouchDrag {
                    id: ev.id,
                    start: ev.position,
                    start_left: get_left_px(&node),
                });
            }
            TouchPhase::Moved => {
                if let Some(td) = drag.touch
                    && td.id == ev.id
                {
                    let dx = ev.position.x - td.start.x;
                    let mut left = td.start_left + dx;

                    if left > 0.0 {
                        rotate_last_to_front(track_e, children, &mut commands);
                        left -= view_width;
                        slider.current =
                            (slider.current + slider.page_count - 1) % slider.page_count;
                        drag.touch.as_mut().unwrap().start_left -= view_width;
                    } else if left < -view_width {
                        rotate_first_to_end(track_e, children, &mut commands);
                        left += view_width;
                        slider.current = (slider.current + 1) % slider.page_count;
                        drag.touch.as_mut().unwrap().start_left += view_width;
                    }

                    node.left = Val::Px(left);
                }
            }
            TouchPhase::Ended | TouchPhase::Canceled => {
                if drag.touch.take().is_some() {
                    let left = get_left_px(&node);
                    
                    if left > 0.0 {
                        // Dragged left (previous page visible)
                        let frac = left / view_width;
                        if frac > 0.05 {
                            // Complete transition to previous page
                            start_back_tween(track_e, &mut node, &mut slider, &mut commands);
                        } else {
                            // Snap back to current (cancel)
                            start_prev_cancel_tween(track_e, children, &mut node, &mut slider, view_width, &mut commands);
                        }
                    } else {
                        // Dragged right (next page visible)
                        let frac = -left / view_width;
                        if frac > 0.05 {
                            // Complete transition to next page
                            start_next_tween(track_e, &mut node, &mut slider, view_width, &mut commands);
                        } else {
                            // Snap back to current
                            start_back_tween(track_e, &mut node, &mut slider, &mut commands);
                        }
                    }
                }
            }
        }
    }

    // mouse
    for ev in ev_cursor.read() {
        if mouse_buttons.pressed(MouseButton::Left) {
            if drag.mouse.is_none() {
                drag.mouse = Some(MouseDrag {
                    start: ev.position,
                    start_left: get_left_px(&node),
                });
            } else if let Some(md) = drag.mouse {
                let dx = ev.position.x - md.start.x;
                let mut left = md.start_left + dx;

                if left > 0.0 {
                    rotate_last_to_front(track_e, children, &mut commands);
                    left -= view_width;
                    slider.current = (slider.current + slider.page_count - 1) % slider.page_count;
                    drag.mouse.as_mut().unwrap().start_left -= view_width;
                } else if left < -view_width {
                    rotate_first_to_end(track_e, children, &mut commands);
                    left += view_width;
                    slider.current = (slider.current + 1) % slider.page_count;
                    drag.mouse.as_mut().unwrap().start_left += view_width;
                }

                node.left = Val::Px(left);
            }
        }
    }

    if mouse_buttons.just_released(MouseButton::Left) && drag.mouse.take().is_some() {
        let left = get_left_px(&node);
        
        if left > 0.0 {
            // Dragged left (previous page visible)
            let frac = left / view_width;
            if frac > 0.05 {
                // Complete transition to previous page
                start_back_tween(track_e, &mut node, &mut slider, &mut commands);
            } else {
                // Snap back to current (cancel)
                start_prev_cancel_tween(track_e, children, &mut node, &mut slider, view_width, &mut commands);
            }
        } else {
            // Dragged right (next page visible)
            let frac = -left / view_width;
            if frac > 0.05 {
                // Complete transition to next page
                start_next_tween(track_e, &mut node, &mut slider, view_width, &mut commands);
            } else {
                // Snap back to current
                start_back_tween(track_e, &mut node, &mut slider, &mut commands);
            }
        }
    }

    if mouse_buttons.just_pressed(MouseButton::Left)
        && drag.mouse.is_none()
        && let Ok(win) = windows.single()
        && let Some(pos) = win.cursor_position()
    {
        drag.mouse = Some(MouseDrag {
            start: pos,
            start_left: get_left_px(&node),
        });
    }
}

fn process_pending_steps(
    mut q: Query<(Entity, &Children, &mut Node, &mut Slider, Option<&TweenAnim>), With<PageTrack>>,
    mut commands: Commands,
) {
    let Ok((entity, children, mut node, mut slider, animator)) = q.single_mut() else {
        return;
    };

    if animator.is_some() || slider.pending_steps == 0 {
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

fn on_tween_completed(
    mut ev: MessageReader<AnimCompletedEvent>,
    mut q: Query<(Entity, &Children, &mut Node, &mut Slider), With<PageTrack>>,
    mut commands: Commands,
) {
    for e in ev.read() {
        if let Ok((entity, children, mut node, mut slider)) = q.get_mut(e.anim_entity) {
            if slider.post_action == PostAction::RotateFirstToEndResetToZero {
                rotate_first_to_end(entity, children, &mut commands);
                slider.current = (slider.current + 1) % slider.page_count;
                node.left = Val::Px(0.0);
            }
            slider.post_action = PostAction::None;
        }
    }
}

fn handle_window_resize(
    mut eview_width: MessageReader<WindowResized>,
    mut q: Query<(&Children, &mut Node, &mut Slider), With<PageTrack>>,
    mut q_page: Query<&mut Node, (With<Page>, Without<PageTrack>)>,
) {
    let mut new_width: Option<f32> = None;
    for ev in eview_width.read() {
        new_width = Some(ev.width);
    }
    if let Some(view_w) = new_width {
        let Ok((children, mut track_node, mut slider)) = q.single_mut() else {
            return;
        };
        slider.view_w = view_w;
        track_node.width = Val::Px(slider.page_count as f32 * view_w);

        for child in children {
            if let Ok(mut n) = q_page.get_mut(*child) {
                n.width = Val::Px(view_w);
            }
        }

        track_node.left = Val::Px(0.0);
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

    let tween = Tween::new(
        EaseFunction::CubicOut,
        std::time::Duration::from_millis(200),
        NodeLeftLens { start, end },
    );
    commands.entity(track).insert(TweenAnim::new(tween));
}

fn start_back_tween(track: Entity, node: &mut Node, slider: &mut Slider, commands: &mut Commands) {
    let start = get_left_px(node);
    let end = 0.0;
    slider.post_action = PostAction::None;

    let tween = Tween::new(
        EaseFunction::CubicOut,
        std::time::Duration::from_millis(200),
        NodeLeftLens { start, end },
    );
    commands.entity(track).insert(TweenAnim::new(tween));
}

fn start_prev_cancel_tween(
    track: Entity,
    children: &Children,
    node: &mut Node,
    slider: &mut Slider,
    _view_width: f32,
    commands: &mut Commands,
) {
    // Cancel the drag - need to rotate back and reset position
    rotate_first_to_end(track, children, commands);
    slider.current = (slider.current + 1) % slider.page_count;
    node.left = Val::Px(0.0);
    slider.post_action = PostAction::None;
}

fn rotate_first_to_end(track: Entity, children: &Children, commands: &mut Commands) {
    if children.is_empty() {
        return;
    }
    let first = children[0];
    commands.entity(track).remove_children(&[first]);
    commands.entity(track).add_children(&[first]);
}

fn rotate_last_to_front(track: Entity, children: &Children, commands: &mut Commands) {
    if children.is_empty() {
        return;
    }
    let last = *children.last().unwrap();
    commands.entity(track).remove_children(&[last]);
    commands.entity(track).insert_children(0, &[last]);
}
