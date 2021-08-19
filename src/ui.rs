use crate::frequency_tracker::FrequencyTracker;
use crate::prelude::*;
use bevy::{prelude::*, render::camera::Camera};
use bevy_prototype_lyon::{entity::ShapeBundle, prelude::*};
use lyon_tessellation::path::Path;
use std::sync::Arc;

struct Tracker(Arc<FrequencyTracker>);
struct UpdateFrequencyCurvePathTimer(Timer);
struct UpdateCounter(usize);
struct ShadePlank;

pub fn start(tracker: Arc<FrequencyTracker>) {
    App::build()
        .insert_resource(Msaa { samples: 8 })
        .insert_resource(ClearColor(Color::rgb(1., 1., 1.)))
        .insert_resource(Tracker(tracker))
        .insert_resource(UpdateCounter(0))
        .insert_resource(UpdateFrequencyCurvePathTimer(Timer::from_seconds(
            REPORT_FREQUENCY_AFTER_MS as f32 / 1000.,
            true, // repetitive
        )))
        .add_plugins(DefaultPlugins)
        .add_plugin(ShapePlugin)
        .add_startup_system(setup.system())
        .add_system(redraw_frequency_curve.system())
        .add_system(slide_camera_and_shade_plank.system())
        .run();
}

fn setup(mut commands: Commands, mut materials: ResMut<Assets<ColorMaterial>>) {
    let mut cam = OrthographicCameraBundle::new_2d();
    cam.transform.translation.x = 200.;
    cam.transform.translation.y = hz_to_y(HIGHEST_FREQUENCY_OF_INTEREST / 2.);
    commands.spawn_bundle(cam);

    let mut plank = SpriteBundle {
        material: materials.add(Color::rgb(1.0, 0.0, 1.0).into()),
        sprite: Sprite::new(Vec2::new(30.0, 5_000.)),
        ..Default::default()
    };
    plank.transform.translation.z = 1.0;
    commands.spawn_bundle(plank).insert(ShadePlank);

    let mut history = FrequencyCurveHistory::new();
    history.move_to(Vec2::new(0.0, 0.0));
    commands
        .spawn_bundle(history.shape())
        .insert(FrequencyCurve);
    commands.insert_resource(history);

    [-0.2, HIGHEST_FREQUENCY_OF_INTEREST].iter().for_each(|f| {
        let y = hz_to_y(*f);
        commands.spawn_bundle(GeometryBuilder::build_as(
            &shapes::Line(Vec2::new(0., y), Vec2::new(1_000_000., y)),
            ShapeColors::new(Color::GRAY),
            DrawMode::Stroke(StrokeOptions::default().with_line_width(3.0)),
            Transform::default(),
        ));
    });
}

#[derive(Debug)]
enum PathCommand {
    MoveTo(Vec2),
    QuadraticBezier(Vec2, Vec2),
}

struct FrequencyCurve;

struct FrequencyCurveHistory(Vec<PathCommand>);
impl FrequencyCurveHistory {
    fn new() -> Self {
        Self(vec![])
    }

    fn move_to(&mut self, dest: Vec2) {
        self.0.push(PathCommand::MoveTo(dest));
    }

    fn quadratic_bezier_to(&mut self, curvature: Vec2, dest: Vec2) {
        self.0.push(PathCommand::QuadraticBezier(curvature, dest));
    }

    fn current_position(&self) -> Vec2 {
        self.0
            .iter()
            .last()
            .map(|cmd| {
                if let PathCommand::MoveTo(point) = cmd {
                    *point
                } else {
                    panic!(
                        "Cannot get current position because {:?} isn't MoveTo",
                        cmd
                    )
                }
            })
            .unwrap_or_else(|| Vec2::new(0., 0.))
    }

    fn build_path(&self) -> Path {
        let mut p = PathBuilder::new();
        for cmd in &self.0 {
            match cmd {
                PathCommand::MoveTo(dest) => p.move_to(*dest),
                PathCommand::QuadraticBezier(curvature, dest) => {
                    p.quadratic_bezier_to(*curvature, *dest)
                }
            };
        }

        p.build()
    }

    fn shape(&self) -> ShapeBundle {
        GeometryBuilder::build_as(
            &self.build_path(),
            ShapeColors::new(Color::BLACK),
            DrawMode::Stroke(StrokeOptions::default().with_line_width(3.0)),
            Transform::default(),
        )
    }
}

fn slide_camera_and_shade_plank(
    time: Res<Time>,
    update_counter: ResMut<UpdateCounter>,
    mut query: QuerySet<(
        Query<&mut Transform, With<Camera>>,
        Query<&mut Transform, With<ShadePlank>>,
    )>,
) {
    let dx = |x: f32, offset: f32| -> f32 {
        let target = (update_counter.0 as f32 - offset) * 20. - 5.;
        (target - x).max(0.)
            * (time.delta().as_millis() as f32
                / REPORT_FREQUENCY_AFTER_MS as f32)
    };

    if update_counter.0 > 6 {
        let cam = &mut query
            .q0_mut()
            .single_mut()
            .expect("Cannot get camera")
            .translation;
        cam.x += dx(cam.x, 6.0);
    }

    let plank = &mut query
        .q1_mut()
        .single_mut()
        .expect("Cannot get shade plank")
        .translation;
    plank.x += dx(plank.x, -1.5);
}

fn redraw_frequency_curve(
    mut cmd: Commands,
    time: Res<Time>,
    mut timer: ResMut<UpdateFrequencyCurvePathTimer>,
    tracker: Res<Tracker>,
    mut update_counter: ResMut<UpdateCounter>,
    mut history: ResMut<FrequencyCurveHistory>,
    existing_curve: Query<Entity, With<FrequencyCurve>>,
) {
    if timer.0.tick(time.delta()).just_finished() {
        if let Ok(entity) = existing_curve.single() {
            // remove currently drawn path
            cmd.entity(entity)
                .remove_bundle::<ShapeBundle>()
                .remove::<FrequencyCurve>();
        }

        let (x1, y1) = {
            let c = history.current_position();
            (c.x, c.y)
        };

        // get latest frequency reading and calculate new y coord based on
        // that, and since x is time we just increment
        let y2 = if let Some(hz) = tracker.0.calculate_latest() {
            hz_to_y(hz)
        } else {
            // slowly go down with frequency since there isn't any useful
            // reading
            y1 * 0.9
        };
        let x2 = update_counter_to_x(update_counter.0 + 1);
        // this is where our new curve must end up
        let dest = Vec2::new(x2, y2);
        // we draw a small sinusoid instead of a straight line
        let phase = if update_counter.0 % 2 == 0 { 1. } else { -1. };

        let diff = y1 - y2;
        // if new frequency is different than prev by significant amount (I.)
        // then we curve the connection more drastically than if both approx
        // similar (II.)
        let curvature = if diff.abs() > 50.0 {
            // I.

            // if new frequency higher then we're going up, but since the
            // curve oscillates (based on phase), we must adjust the shape
            // accordingly so that we don't get a sharp corner
            if diff.is_sign_negative() {
                if phase == -1.0 {
                    Vec2::new(x2, y1 - 30.0)
                } else {
                    Vec2::new(x1, y2 + 30.0)
                }
            } else {
                if phase == -1.0 {
                    Vec2::new(x1, y2 - 30.0)
                } else {
                    Vec2::new(x2, y1 + 30.0)
                }
            }
        } else {
            // II.
            Vec2::new((x2 + x1) / 2., (y2 + y1) / 2. + 10.0 * phase)
        };

        // apply new curve to history
        history.quadratic_bezier_to(curvature, dest);
        history.move_to(dest);

        // re-insert new curve mesh
        cmd.spawn_bundle(history.shape()).insert(FrequencyCurve);

        // next iteration will consider next point on x axis
        update_counter.0 += 1;
    }
}

fn update_counter_to_x(counter: usize) -> f32 {
    counter as f32 * 20.
}

fn hz_to_y(hz: f32) -> f32 {
    hz * 100.
}
