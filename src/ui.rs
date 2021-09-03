//! TODO: This module is yet to be cleaned up as it's still WIP.

use crate::frequency_tracker::FrequencyTracker;
use crate::prelude::*;
use bevy::{prelude::*, render::camera::Camera};
use bevy_prototype_lyon::{entity::ShapeBundle, prelude::*};
use lyon_tessellation::path::Path;
use std::sync::Arc;

struct Tracker(Arc<FrequencyTracker>);

/// Defines often we sample frequency readings and therefore update new target
/// for the y coordinate of the curve.
struct SampleNextY(Timer);

/// We increment this counter every time the [`SampleNextY`] interval timer
/// is finished.
struct FrequencyReadingsCounter(usize);

/// The shape of the curve can be queried with this tag.
struct FrequencyCurve;

/// Everytime [`SampleNextY`] finishes, we rebuild the whole curve. We therefore
/// need to keep track of the few dozens latests constituent shapes which
/// create the output curve.
struct FrequencyCurveHistory(Vec<PathCommand>);

/// The frequency curve is redrawn every now and then. We keep the tip hidden
/// under a white plane and with each tick slightly move the plane. This creates
/// the illusion that the curve is drawn continuously.
struct ShadePlane;

/// Defines how long each new bit of the curve is.
const SINGLE_READING_TO_PX: f32 = 20.0;

#[derive(Debug)]
enum PathCommand {
    MoveTo(Vec2),
    QuadraticBezier(Vec2, Vec2),
}

pub fn start(tracker: Arc<FrequencyTracker>) {
    App::build()
        .insert_resource(Msaa { samples: 8 })
        .insert_resource(ClearColor(Color::rgb(1., 1., 1.)))
        .insert_resource(Tracker(tracker))
        .insert_resource(FrequencyReadingsCounter(0))
        .insert_resource(SampleNextY::new(
            REPORT_FREQUENCY_AFTER_MS as f32 / 1000.,
        ))
        .add_plugins(DefaultPlugins)
        .add_plugin(ShapePlugin)
        .add_startup_system(setup.system())
        .add_system(redraw_frequency_curve.system())
        .add_system(slide_camera_and_shade_plane.system())
        .run();
}

fn setup(mut commands: Commands, mut materials: ResMut<Assets<ColorMaterial>>) {
    // camera looks directly towards the [1, 1, 0] plane
    let mut cam = OrthographicCameraBundle::new_2d();
    cam.transform.translation.x = 200.;
    cam.transform.translation.y = hz_to_y(HIGHEST_FREQUENCY_OF_INTEREST / 2.);
    commands.spawn_bundle(cam);

    // a large white plane which obstructs tip of the curve, see the
    // [`ShadePlane`] for more info
    let mut plane = SpriteBundle {
        material: materials.add(Color::rgb(1.0, 0.0, 1.0).into()),
        sprite: Sprite::new(Vec2::new(30.0, 5_000.)),
        ..Default::default()
    };
    plane.transform.translation.z = 1.0;
    commands.spawn_bundle(plane).insert(ShadePlane);

    let mut history = FrequencyCurveHistory::new();
    history.move_to(Vec2::new(0.0, 0.0));
    commands
        .spawn_bundle(history.shape())
        .insert(FrequencyCurve);
    commands.insert_resource(history);

    // draws two lines which are the min and max limit for any observed
    // frequency
    [-0.2, HIGHEST_FREQUENCY_OF_INTEREST].iter().for_each(|f| {
        let y = hz_to_y(*f);
        commands.spawn_bundle(GeometryBuilder::build_as(
            // TODO: move the lines instead of this hack
            &shapes::Line(Vec2::new(0., y), Vec2::new(1_000_000., y)),
            ShapeColors::new(Color::GRAY),
            DrawMode::Stroke(StrokeOptions::default().with_line_width(3.0)),
            Transform::default(),
        ));
    });
}

fn slide_camera_and_shade_plane(
    time: Res<Time>,
    readings_counter: ResMut<FrequencyReadingsCounter>,
    mut query: QuerySet<(
        Query<&mut Transform, With<Camera>>,
        Query<&mut Transform, With<ShadePlane>>,
    )>,
) {
    let dx = |x: f32, offset: f32| -> f32 {
        // remove 5pxs to always slide slightly behind the curve to avoid
        // exposing
        // TODO
        let target_x =
            frequency_readings_count_to_x((readings_counter.as_f32() - offset) as usize)
                - 5.0;
        let nudge_by = (target_x - x).max(0.);

        nudge_by
            * (time.delta().as_millis() as f32
                / REPORT_FREQUENCY_AFTER_MS as f32)
    };

    if readings_counter.as_usize() > 6 {
        // start updating the camera only after a few updates
        let cam = &mut query
            .q0_mut()
            .single_mut()
            .expect("Cannot get camera")
            .translation;
        cam.x += dx(cam.x, 6.0);
    }

    let plane = &mut query
        .q1_mut()
        .single_mut()
        .expect("Cannot get shade plane")
        .translation;
    plane.x += dx(plane.x, -1.5);
}

fn redraw_frequency_curve(
    mut cmd: Commands,
    time: Res<Time>,
    mut timer: ResMut<SampleNextY>,
    tracker: Res<Tracker>,
    mut readings_counter: ResMut<FrequencyReadingsCounter>,
    mut history: ResMut<FrequencyCurveHistory>,
    existing_curve: Query<Entity, With<FrequencyCurve>>,
) {
    if !timer.0.tick(time.delta()).just_finished() {
        return;
    }

    // remove currently drawn path because we will rerender
    if let Ok(entity) = existing_curve.single() {
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
    let x2 = frequency_readings_count_to_x(readings_counter.as_usize() + 1);
    // this is where our new curve must end up
    let dest = Vec2::new(x2, y2);
    // we draw a small sinusoid instead of a straight line
    let phase = if readings_counter.as_usize() % 2 == 0 {
        1.
    } else {
        -1.
    };

    let diff = y1 - y2;
    // if new frequency is different than prev by significant amount (I.)
    // then we curve the connection more drastically than if both approx
    // similar (II.)
    const THRESHOLD_HZ_DIFF: f32 = 50.0;
    let curvature = if diff.abs() > THRESHOLD_HZ_DIFF {
        // I.

        // when we overshoot the target by a little the curve looks smoother
        const OVERSHOOT_Y_PXS: f32 = 30.0;

        // if new frequency higher then we're going up, but since the
        // curve oscillates (based on phase), we must adjust the shape
        // accordingly so that we don't get a sharp corner
        if diff.is_sign_negative() {
            if phase == -1.0 {
                Vec2::new(x2, y1 - OVERSHOOT_Y_PXS)
            } else {
                Vec2::new(x1, y2 + OVERSHOOT_Y_PXS)
            }
        } else {
            if phase == -1.0 {
                Vec2::new(x1, y2 - OVERSHOOT_Y_PXS)
            } else {
                Vec2::new(x2, y1 + OVERSHOOT_Y_PXS)
            }
        }
    } else {
        // II.
        // draws sinusoids
        Vec2::new((x2 + x1) / 2., (y2 + y1) / 2. + 10.0 * phase)
    };

    // apply new curve to history
    history.quadratic_bezier_to(curvature, dest);
    history.move_to(dest);

    // re-insert new curve mesh
    cmd.spawn_bundle(history.shape()).insert(FrequencyCurve);

    // next iteration will consider next point on x axis
    readings_counter.0 += 1;
}

fn frequency_readings_count_to_x(counter: usize) -> f32 {
    // each time we draw a new bit of the curve, we add
    counter as f32 * SINGLE_READING_TO_PX
}

fn hz_to_y(hz: f32) -> f32 {
    hz * 100.
}

impl FrequencyReadingsCounter {
    fn as_usize(&self) -> usize {
        self.0
    }

    fn as_f32(&self) -> f32 {
        self.0 as f32
    }
}

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

impl SampleNextY {
    fn new(seconds: f32) -> Self {
        let repeat = true;
        Self(Timer::from_seconds(seconds, repeat))
    }
}
