use std::{
    f32::{self, consts::PI},
    time::Instant,
};

use eframe::{App, NativeOptions};
use egui::{
    Color32, Pos2, Shape, Stroke, Ui, Vec2, ViewportBuilder,
    ahash::{HashMap, HashMapExt},
    epaint::CircleShape,
};
use rand::{random_bool, random_range};

struct Pandemic {
    // Public params
    infection_prob: f32,
    infection_time: f32,
    death_prob: f32,
    step_speed: f32,

    // Data
    grid: SpatialGrid,
    last_frame_time: Instant,
}

const X_MAX: i32 = 160;
const Y_MAX: i32 = 100;
const X_MAX_FLOAT: f32 = X_MAX as f32;
const Y_MAX_FLOAT: f32 = Y_MAX as f32;

impl Pandemic {
    fn new(infected: usize, total: usize) -> Self {
        Self {
            infection_prob: 0.7,
            infection_time: 14000.0,
            death_prob: 0.2,
            step_speed: 2.0,

            grid: SpatialGrid::new_with_capacity(infected, total),
            last_frame_time: Instant::now(),
        }
    }

    fn step(&mut self) {
        let rad = 0.3;
        let rad_sq = rad * rad;
        let frame_time = self.last_frame_time.elapsed().as_millis() as f32 * self.step_speed;
        self.last_frame_time = Instant::now();

        // Iterate over rows and cols
        for x_pos in 0..X_MAX {
            for y_pos in 0..Y_MAX {
                let mut people_to_move = Vec::new();
                // Get everyone in grid element
                if let Some(people) = self.grid.0.get_mut(&(x_pos, y_pos)) {
                    // Step each individual
                    let dist_to_move = 0.01 * frame_time;
                    people_to_move.extend(people.extract_if(.., |person| {
                        // Step direction
                        let pos = &mut person.pos;
                        let dir = person.direction;
                        let (x_comp, y_comp) = f32::sin_cos(dir);
                        pos.x = pos.x + (dist_to_move * x_comp);
                        pos.y = pos.y + (dist_to_move * y_comp);

                        // If OOB, flip direction & reflect back
                        if pos.x < 0.0 {
                            pos.x = -pos.x;
                            person.direction = -dir;
                        } else if pos.x > X_MAX_FLOAT {
                            pos.x = 2.0 * X_MAX_FLOAT - pos.x;
                            person.direction = -dir;
                        }
                        if pos.y < 0.0 {
                            pos.y = -pos.y;
                            person.direction = PI - dir;
                        } else if pos.y > Y_MAX_FLOAT {
                            pos.y = 2.0 * Y_MAX_FLOAT - pos.y;
                            person.direction = PI - dir;
                        }

                        if let InfectionState::Infected(t) = person.state {
                            // Chance to die
                            let dead = random_bool(
                                (self.death_prob * (frame_time / self.infection_time)) as f64,
                            );
                            if dead {
                                person.state = InfectionState::Dead;
                                return true;
                            }

                            // Update infection time
                            let new_infection_time = t + frame_time;
                            person.state = if new_infection_time > self.infection_time {
                                InfectionState::Recovered
                            } else {
                                InfectionState::Infected(new_infection_time)
                            };
                        }

                        // Do not retain if out of grid element
                        let grid_x = pos.x as i32;
                        let grid_y = pos.y as i32;
                        grid_x != x_pos || grid_y != y_pos
                    }));

                    // Infection testing
                    for i in 0..people.len() {
                        let (left, right) = people.split_at_mut(i + 1);
                        let person1 = &mut left[i];

                        for j in 0..right.len() {
                            let person2 = &mut right[j];

                            // Close enough to infect
                            let (dx, dy) =
                                (person2.pos.x - person1.pos.x, person2.pos.y - person1.pos.y);
                            let dist_sq = dx.powi(2) + dy.powi(2);
                            if dist_sq.abs() < rad_sq {
                                match (
                                    &person1.state,
                                    &person2.state,
                                    random_bool(self.infection_prob as f64),
                                ) {
                                    (
                                        InfectionState::Healthy,
                                        InfectionState::Infected(_),
                                        true,
                                    ) => {
                                        person1.state = InfectionState::Infected(0.0);
                                    }
                                    (
                                        InfectionState::Infected(_),
                                        InfectionState::Healthy,
                                        true,
                                    ) => {
                                        person2.state = InfectionState::Infected(0.0);
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                }
                // If people need to be moved, move them
                for person in people_to_move {
                    if person.state == InfectionState::Dead {
                        continue;
                    }
                    self.grid
                        .0
                        .entry((person.pos.x as i32, person.pos.y as i32))
                        .or_default()
                        .push(person);
                }
            }
        }
    }
}

type GridMap = HashMap<(i32, i32), Vec<Person>>;
struct SpatialGrid(GridMap);
impl SpatialGrid {
    fn new_with_capacity(infected: usize, total: usize) -> Self {
        // Generate random data for new person
        fn rand_person() -> (f32, f32, f32) {
            let (x, y) = (
                random_range(1.0..X_MAX_FLOAT),
                random_range(1.0..Y_MAX_FLOAT),
            );
            let direction = random_range(0.0..(2.0 * f32::consts::PI));
            (x, y, direction)
        }

        let mut map: GridMap = HashMap::with_capacity(total);

        for _ in 0..infected {
            let (x, y, direction) = rand_person();
            map.entry((x as i32, y as i32)).or_default().push(Person {
                pos: Pos2 { x, y },
                direction,
                state: InfectionState::Infected(0.0),
            });
        }

        for _ in 0..(total - infected) {
            let (x, y, direction) = rand_person();
            map.entry((x as i32, y as i32)).or_default().push(Person {
                pos: Pos2 { x, y },
                direction,
                state: InfectionState::Healthy,
            });
        }

        Self(map)
    }

    fn render(&self, ui: &mut Ui) {
        ui.painter().extend(self.0.values().flatten().map(|person| {
            Shape::Circle(CircleShape {
                center: Pos2 {
                    x: person.pos.x * 10.,
                    y: person.pos.y * 10.,
                },
                radius: 5.0,
                fill: match person.state {
                    InfectionState::Healthy => Color32::GREEN,
                    InfectionState::Infected(_) => Color32::RED,
                    InfectionState::Recovered => Color32::PURPLE,
                    InfectionState::Dead => unreachable!("Dead people should be removed before render!"),
                },
                stroke: Stroke::NONE,
            })
        }));
    }
}

#[derive(Clone, Copy)]
struct Person {
    pos: Pos2,
    direction: f32,
    state: InfectionState,
}

#[derive(Clone, Copy, PartialEq)]
enum InfectionState {
    Healthy,
    Infected(f32),
    Recovered,
    Dead,
}

impl App for Pandemic {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            self.grid.render(ui);
            self.step();
        });

        ctx.request_repaint();
    }
}

fn main() {
    let native_options = NativeOptions {
        viewport: ViewportBuilder::default()
            .with_inner_size(Vec2 { x: 1600., y: 1000. })
            .with_resizable(false),
        ..Default::default()
    };
    eframe::run_native(
        "pandemic",
        native_options,
        Box::new(|_| Ok(Box::new(Pandemic::new(3, 500)))),
    )
    .unwrap();
}
