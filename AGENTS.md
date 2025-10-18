Plan

- Keep your existing sections and tone. Add a Bevy 0.17-focused, LLM-oriented migration layer: what to change, how to review, and how to verify.
- Provide concise, copy-pasteable Rust snippets (nightly, Bevy 0.17), ECS-first, early-return.
- Include Nu shell verification steps.
- Do not refactor beyond scope; only add guidance that agents can follow per-PR.

Proposed AGENTS.md (merged with Bevy 0.17)

Title: Agent Playbook for Bevy 0.17 Migrations

Scope and hard rules

- Preserve gameplay, physics, balance.
- Verify often: cargo clippy --workspace --all-features
- Match local style and module layout. ECS-first, early return.
- Never: destructive git, just, cargo run, cargo clean, DB changes, deploys.
- Prefer small, isolated PRs. Modify only requested areas.
- Use migration patterns consistent with existing code.

Minimal workflow

- For any change:
  - Build and clippy for the entire workspace with all features.
  - Add focused tests/smoke checks when touching behavior surfaces.
  - Keep diffs minimal; avoid opportunistic refactors.

High-priority migration rules (LLM-ready)

- Messages vs Events
  - Buffered events are now Messages. Use MessageWriter/MessageReader/Messages<M>. Old send/send_batch => write/write_batch.
  - Observers use Event with On<E>. Entity-targeted events derive EntityEvent.

  Replace:

  ```rust
  #[derive(Event)] struct Hit(u32);
  fn deal(mut w: EventWriter<Hit>) { w.send(Hit(1)); }
  ```

  With:

  ```rust
  use bevy::prelude::*;

  #[derive(Message)]
  struct Hit(u32);

  fn deal(mut w: MessageWriter<Hit>) {
      w.write(Hit(1));
  }
  ```

  Observers (entity-targeted):

  ```rust
  use bevy::prelude::*;

  #[derive(EntityEvent, Clone)]
  struct Explode {
      entity: Entity,
  }

  fn trigger_explode(world: &mut World, e: Entity) {
      world.trigger(Explode { entity: e });
  }

  fn observe() -> impl FnMut(On<Explode>) {
      |ev: On<Explode>| {
          let e = ev.entity; // or ev.event_target()
          info!("exploded: {e:?}");
      }
  }
  ```

- ECS API changes
  - Query::single()/single_mut() return Result. Handle errors, don’t unwrap.
  - World and mapping APIs often return Result.

  Example:

  ```rust
  fn player_cam(
      q: Query<&Transform, With<Player>>,
  ) {
      let Ok(t) = q.single() else { return };
      // use t
  }
  ```

  Commands API changes:
  - EntityCommands::push_children -> add_children
  - Parent -> ChildOf relationship APIs; prefer ChildOf and add_children.

- Scheduling and states
  - If not using DefaultPlugins, add StatesPlugin.
  - OnEnter runs immediately on state init. If you relied on Startup-before-OnEnter ordering, add a Setup state.

  NextState:

  ```rust
  use bevy::prelude::*;
  use bevy_state::state::{in_state, NextState, States, StatesPlugin};

  #[derive(States, Clone, Eq, PartialEq, Hash, Debug, Default)]
  enum GameState { #[default] Loading, Playing }

  fn go_playing(mut next: ResMut<NextState<GameState>>) {
      next.set(GameState::Playing); // becomes Pending(Playing) internally
  }

  fn guard(_q: Query<(), With<Something>>, _in: bevy_state::condition::InState<GameState>) {}
  ```

- UI and Text
  - NodeBundle, TextBundle removed. Use Node, Text, TextLayout, TextFont, TextColor.
  - Tint images via UiImage::with_color. BackgroundColor doesn’t tint images.
  - Anchors moved; use Anchor::{BOTTOM_LEFT,...} associated constants.

  Example UI:

  ```rust
  use bevy::prelude::*;

  fn ui(mut cmd: Commands, assets: Res<AssetServer>) {
      cmd.spawn((
          Node {
              width: Val::Px(300.0),
              height: Val::Px(80.0),
              ..default()
          },
          BackgroundColor(Color::srgb(0.12, 0.12, 0.12)),
      ))
      .with_children(|p| {
          p.spawn((
              Text::new("Hello"),
              TextFont::from(assets.load("fonts/FiraSans-Bold.ttf")),
              TextLayout::default(),
              TextColor(Color::WHITE),
          ));
      });
  }
  ```

- Rendering and camera
  - Camera2dBundle/Camera3dBundle removed; insert Camera2d/Camera3d + Camera (and others).
  - Msaa per camera via Msaa component.
  - Clear color uses Option<LinearRgba> via Camera’s output.

  Example 2D camera:

  ```rust
  use bevy::prelude::*;

  fn cam(mut cmd: Commands) {
      cmd.spawn((
          Camera2d,
          Camera {
              order: 0,
              clear_color: Some(Color::LINEAR_RGBA_WHITE),
              ..default()
          },
          Msaa::Sample4,
      ));
  }
  ```

- Sprites, meshes, materials
  - SpriteBundle deprecated; use Sprite directly with TextureAtlas as needed.
  - For meshes: wrap handles with Mesh2d/Mesh3d and MeshMaterial2d/MeshMaterial3d.

  Example sprite:

  ```rust
  use bevy::prelude::*;

  fn sprite(mut cmd: Commands, assets: Res<AssetServer>) {
      cmd.spawn((
          Sprite {
              image: assets.load("player.png"),
              ..default()
          },
          Transform::IDENTITY,
          Anchor::CENTER, // required for Sprite
      ));
  }
  ```

- Assets and loaders
  - Assets::insert/get_or_insert_with return Result.
  - AssetServer::load_state returns LoadState::Failed(error) with info.

  Example load poll:

  ```rust
  use bevy::prelude::*;

  fn check(
      server: Res<AssetServer>,
      handle: Res<Handle<Image>>,
  ) {
      match server.load_state(handle.id()) {
          bevy::asset::LoadState::Loaded => { /* ready */ }
          bevy::asset::LoadState::Failed(e) => {
              error!("asset failed: {e}");
          }
          _ => {}
      }
  }
  ```

- Input and windowing
  - Gamepads are entities; query &Gamepad and use fields.
  - Keyboard text: use KeyboardInput; ReceivedCharacter removed.

- Bevy render reorganization
  - Import camera types from bevy::camera, shader types from bevy::shader, light types from bevy::light, mesh types from bevy::mesh, image types from bevy::image, UI render types from bevy::ui_render, sprite render from bevy::sprite_render.
  - If you used post-process/AA plugins, update imports to bevy::anti_alias and bevy::post_process.

- Observer API changes (rename and ergonomics)
  - Trigger<T> -> On<T>. Lifecycle events: Add/Insert/Replace/Remove/Despawn.

  Example component lifecycle:

  ```rust
  use bevy::prelude::*;

  #[derive(Component)]
  struct Player;

  fn on_add_player() -> impl FnMut(On<Add, Player>) {
      |add: On<Add, Player>| {
          info!("Player added: {:?}", add.entity);
      }
  }
  ```

- System::run returns Result
  - If you manually run systems, unwrap or propagate Result. Provide explicit output type if inference fails.

- Window split components
  - CursorOptions moved to its own component and WindowPlugin field.

  ```rust
  use bevy::prelude::*;

  fn lock_cursor(mut s: Single<&mut CursorOptions, With<PrimaryWindow>>) {
      s.grab_mode = CursorGrabMode::Locked;
  }
  ```

- Entities and manual creation changes
  - Use Entity::from_row_u32(u32) -> Option<Entity>.
  - Prefer Entities::alloc for production code.

- Anchor required on Sprite
  - Always add Anchor to Sprite entities.

- UI rendering order fixes
  - Be mindful: backgrounds, borders, gradients, images, materials, text order is fixed.

- Web builds
  - getrandom 0.3 requires RUSTFLAGS with getrandom_backend on wasm32-unknown-unknown. Configure CI accordingly.

bevy_tween integration notes

- Add DefaultTweenPlugins; mark tweenable entities with AnimationTarget; use target.with(...) interpolators.
- Use Repeat::Infinitely with RepeatStyle::PingPong for reversible loops.
- Drive side effects via observers on TweenEvent<\_>.
- Example (2D parallel tween):
  [keep your provided example as-is; it compiles on 0.17 alongside updated Sprite/Camera2d usage.]

Review checklist (agents)

- Imports
  - If code references old paths (bevy_render camera/shader/mesh/image exports), switch to new crates/modules.
- Events/messages
  - Replace EventWriter/Reader usage for buffered patterns with MessageWriter/Reader.
  - Replace send/send_batch with write/write_batch.
- Observers
  - Rename Trigger to On; OnAdd/OnInsert/OnRemove/OnDespawn to Add/Insert/Remove/Despawn.
  - For entity-targeted events, derive EntityEvent; replace world.trigger_targets with world.trigger.
- ECS queries
  - Replace .single()/.single_mut() unwraps with Ok/Err handling and early return.
- Sprites/UI/text
  - Nodes: Node instead of NodeBundle; Text stack: Text + TextFont + TextLayout + TextColor; Sprite requires Anchor.
  - Image tinting via UiImage::with_color.
- Cameras
  - Replace Camera2dBundle/Camera3dBundle with Camera2d/Camera3d + Camera + Msaa as components.
- Mesh/material
  - Wrap mesh handles in Mesh2d/Mesh3d and materials in MeshMaterial2d/3d.
  - Mesh::merge signature returns Result; handle error.
- Assets
  - Handle Result on Assets::insert/get_or_insert_with; handle Failed in AssetServer::load_state.
- Windows/input
  - Cursor options via CursorOptions component; ReceivedCharacter removal; Gamepads are entities.
- Scheduling
  - Add StatesPlugin if DefaultPlugins absent; double-check OnEnter timing.
- System run
  - If calling System::run, unwrap or propagate Result; specify generic Out when needed.

Example diffs (full code snippets, no placeholders)

1. Replace Event to Message for buffered events

```rust
use bevy::prelude::*;

#[derive(Resource, Default)]
struct Score(u32);

// BEFORE
#[derive(Event)]
struct ScoreDelta(u32);

fn add(mut writer: EventWriter<ScoreDelta>) {
    writer.send(ScoreDelta(5));
}
fn apply(mut reader: EventReader<ScoreDelta>, mut score: ResMut<Score>) {
    for e in reader.read() {
        score.0 += e.0;
    }
}

// AFTER
#[derive(Message)]
struct ScoreDelta(u32);

fn add(mut writer: MessageWriter<ScoreDelta>) {
    writer.write(ScoreDelta(5));
}
fn apply(mut reader: MessageReader<ScoreDelta>, mut score: ResMut<Score>) {
    for e in reader.read() {
        score.0 += e.0;
    }
}
```

2. Observer rename and entity-target event

```rust
use bevy::prelude::*;

#[derive(Component)]
struct Explosive;

#[derive(EntityEvent, Clone)]
struct Explode {
    entity: Entity,
}

fn trigger_explosives(mut q: Query<Entity, With<Explosive>>, mut world: World) {
    for e in &mut q {
        world.trigger(Explode { entity: e });
    }
}

fn observe_explode() -> impl FnMut(On<Explode>) {
    |ev: On<Explode>| {
        info!("Boom: {:?}", ev.entity);
    }
}
```

3. Camera 2D migration

```rust
use bevy::prelude::*;

fn setup_camera(mut cmd: Commands) {
    cmd.spawn((
        Camera2d,
        Camera {
            order: 0,
            clear_color: Some(Color::LINEAR_RGBA_BLACK),
            ..default()
        },
        Msaa::Sample4,
    ));
}
```

4. UI text migration

```rust
use bevy::prelude::*;

fn setup_ui(mut cmd: Commands, assets: Res<AssetServer>) {
    cmd.spawn((
        Node {
            width: Val::Px(600.0),
            height: Val::Px(120.0),
            ..default()
        },
        BackgroundColor(Color::srgb(0.06, 0.06, 0.08)),
    ))
    .with_children(|p| {
        p.spawn((
            Text::new("Bevy 0.17"),
            TextFont::from(assets.load("fonts/FiraSans-Bold.ttf")),
            TextLayout::default(),
            TextColor(Color::WHITE),
        ));
    });
}
```

5. Sprite + Anchor required

```rust
use bevy::prelude::*;

fn spawn_sprite(mut cmd: Commands, assets: Res<AssetServer>) {
    cmd.spawn((
        Sprite {
            image: assets.load("ship.png"),
            ..default()
        },
        Transform::IDENTITY,
        Anchor::CENTER,
    ));
}
```

6. Query single result handling

```rust
use bevy::prelude::*;

#[derive(Component)]
struct Player;

fn move_player(mut q: Query<&mut Transform, With<Player>>) {
    let Ok(mut t) = q.single_mut() else { return };
    t.translation.x += 1.0;
}
```

7. Assets insert errors

```rust
use bevy::{asset::Assets, prelude::*};

fn insert_mesh(mut meshes: ResMut<Assets<Mesh>>, mesh: Mesh) {
    let id = meshes.reserve();
    if let Err(e) = meshes.insert(id, mesh) {
        error!("failed inserting mesh: {e}");
        return;
    }
}
```

8. bevy_tween ping-pong sample (uses 0.17 Sprite/Camera2d and Anchor)

```rust
use std::time::Duration;
use bevy::prelude::*;
use bevy_tween::{
    combinator::{parallel, tween},
    interpolate::{angle_z, scale, sprite_color, translation},
    prelude::*,
    tween::AnimationTarget,
};

fn secs(s: f32) -> Duration { Duration::from_secs_f32(s) }
fn into_color<T: Into<bevy::color::Srgba>>(c: T) -> Color { Color::Srgba(c.into()) }

fn setup(mut commands: Commands, assets: Res<AssetServer>) {
    commands.spawn((Camera2d, Camera::default(), Msaa::Sample4));

    let image = assets.load("square_filled.png");
    let sprite_here = AnimationTarget.into_target();

    commands
        .spawn((
            Sprite { image, ..default() },
            Transform::IDENTITY,
            Anchor::CENTER,
            AnimationTarget,
        ))
        .animation()
        .repeat(Repeat::Infinitely)
        .repeat_style(RepeatStyle::PingPong)
        .insert(parallel((
            tween(
                secs(1.2),
                EaseKind::CubicInOut,
                sprite_here.with(translation(
                    Vec3::new(-300., 0., 0.),
                    Vec3::new(300., 0., 0.),
                )),
            ),
            tween(
                secs(1.2),
                EaseKind::BackInOut,
                sprite_here.with(scale(Vec3::splat(1.), Vec3::splat(2.))),
            ),
            tween(
                secs(1.2),
                EaseKind::Linear,
                sprite_here.with(angle_z(0., std::f32::consts::PI)),
            ),
            tween(
                secs(1.2),
                EaseKind::Linear,
                sprite_here.with(sprite_color(
                    into_color(bevy::color::palettes::css::WHITE),
                    into_color(bevy::color::palettes::css::DEEP_PINK),
                )),
            ),
        )));
}

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, DefaultTweenPlugins))
        .add_systems(Startup, setup)
        .run();
}
```

Linting and verification

- Always run:
  ```nu
  cargo build --workspace --all-features
  cargo clippy --workspace --all-features -- -D warnings
  ```
- If using WASM/web, ensure:
  - Cargo.toml includes getrandom = { version = "0.3", features = ["wasm_js"] } when targeting wasm32-unknown-unknown.
  - CI sets: RUSTFLAGS='--cfg getrandom_backend="wasm_js"' for web builds.

Notes by subsystem that often trip migrations

- Picking and pointer events: Pointer<Pressed>/Pointer<Released> renamed to Pointer<Press>/Pointer<Release>. Original target available via observers’ On::original_event_target() accessors; prefer entity-targeted events with EntityEvent where suitable.
- Anti-aliasing/post-process: Import from bevy::anti_alias and bevy::post_process. TAA moved to DefaultPlugins; component is bevy::anti_alias::taa::TemporalAntiAliasing.
- Window split: set primary_cursor_options in WindowPlugin. CursorIcon types under bevy::window; component-based control.
- UI z-order: ExtractedUiNode.z_order f32; account for fixed draw order when relying on overlays.
- Mesh normals: Defaults changed to angle-weighted. If visuals regressed, switch to area-weighted methods.

Contribution pattern for agents

- One PR per concern. Examples:
  - PR A: Messages API migration in gameplay crate; no behavior change.
  - PR B: Cameras migrated to Camera2d/Camera3d; Msaa per-camera; Anchor on sprites.
  - PR C: UI/Text migration for HUD only; Text stack changes; visual parity check.
- Each PR description includes:
  - Affected modules and why.
  - Exact Bevy 0.17 changes applied.
  - Verification steps and clippy output.
  - Risk assessment and rollback plan.
