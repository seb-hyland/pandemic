use eframe::App;
use egui::{
    Button, Color32, ComboBox, FontId, Frame, Grid, Label, Margin, Pos2, Shape, Slider, Stroke, Ui,
    Vec2,
    ahash::{HashMap, HashMapExt},
    epaint::{CircleShape, TextShape},
};
use rand::{random_bool, random_range};
use std::{
    f32::{self, consts::PI},
    fmt::Display,
};
use web_time::{Duration, Instant};

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    let native_options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(Vec2 { x: 800., y: 700. })
            .with_resizable(true),
        ..Default::default()
    };
    eframe::run_native(
        "pandemic",
        native_options,
        Box::new(|_| Ok(Box::new(Pandemic::new(5, 500)))),
    )
    .unwrap();
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    let web_options = eframe::WebOptions::default();

    wasm_bindgen_futures::spawn_local(async {
        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("Failed to find the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("the_canvas_id was not a HtmlCanvasElement");

        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|_| Ok(Box::new(Pandemic::new(5, 500)))),
            )
            .await;

        // Remove the loading text and spinner:
        if let Some(loading_text) = document.get_element_by_id("loading_text") {
            match start_result {
                Ok(_) => {
                    loading_text.remove();
                }
                Err(e) => {
                    loading_text.set_inner_html(
                        "<p> The app has crashed. See the developer console for details. </p>",
                    );
                    panic!("Failed to start eframe: {e:?}");
                }
            }
        }
    });
}

struct Pandemic {
    // Initial values
    init_infected: usize,
    total: usize,

    // Public params
    infection_prob: f32,
    infection_time_s: f32,
    death_prob: f32,
    step_speed: f32,
    paused: bool,
    graph: GraphOptions,

    // Data
    grid: SpatialGrid,
    last_frame_time: Instant,
    time_elapsed: Duration,

    // Stats
    num_healthy: usize,
    num_infected: usize,
    num_recovered: usize,
    num_dead: usize,
    stats: Vec<PandemicSnapshot>,
}

impl App for Pandemic {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::TopBottomPanel::bottom("info_panel")
            .exact_height(450.)
            .show(ctx, |ui| {
                egui::SidePanel::left("params")
                    .exact_width(250.)
                    .show_inside(ui, |ui| self.params_ui(ui));

                Frame::new()
                    .outer_margin(Margin::symmetric(20, 30))
                    .show(ui, |ui| {
                        self.graph_ui(ui);
                    });
            });

        egui::CentralPanel::default().show(ctx, |ui| {
            self.grid.render(ui);
            if !self.paused {
                self.step();
            }
        });

        // Re-render (hot loop)
        ctx.request_repaint();
    }
}

const X_MAX: i32 = 80;
const Y_MAX: i32 = 50;
const X_MAX_FLOAT: f32 = X_MAX as f32;
const Y_MAX_FLOAT: f32 = Y_MAX as f32;

impl Pandemic {
    fn new(infected: usize, total: usize) -> Self {
        Self {
            init_infected: infected,
            total,

            infection_prob: 0.5,
            infection_time_s: 14.0,
            death_prob: 0.1,
            step_speed: 1.0,
            paused: false,
            graph: GraphOptions::Infected,

            grid: SpatialGrid::new_with_capacity(infected, total),
            last_frame_time: Instant::now(),
            time_elapsed: Duration::ZERO,

            num_healthy: total - infected,
            num_infected: infected,
            num_recovered: 0,
            num_dead: 0,
            stats: Vec::new(),
        }
    }

    fn params_ui(&mut self, ui: &mut Ui) {
        ui.add_space(15.);

        ui.heading("Controls");
        Grid::new("playback_keys").show(ui, |ui| {
            if ui.add(Button::new("⏸")).clicked() {
                self.paused = true;
            };
            if ui.add(Button::new("▶")).clicked() {
                self.step_speed = 1.0;
                self.paused = false;
                self.last_frame_time = Instant::now();
            };
            if ui.add(Button::new("▶▶")).clicked() {
                self.step_speed = 2.0;
                self.paused = false;
                self.last_frame_time = Instant::now();
            };
            if ui.add(Button::new("▶▶▶")).clicked() {
                self.step_speed = 4.0;
                self.paused = false;
                self.last_frame_time = Instant::now();
            };
            if ui.add(Button::new("⟳")).clicked() {
                let infection_prob = self.infection_prob;
                let infection_time_s = self.infection_time_s;
                let death_prob = self.death_prob;

                *self = Self::new(self.init_infected, self.total);
                self.infection_prob = infection_prob;
                self.infection_time_s = infection_time_s;
                self.death_prob = death_prob;
                self.paused = true;
            }
        });
        ui.add_space(15.);

        ui.heading("Initial conditions");
        ui.add(Label::new("Initial infected"));
        ui.add(Slider::new(&mut self.init_infected, 0..=1000));

        ui.add(Label::new("Total people"));
        ui.add(Slider::new(&mut self.total, 0..=10000));
        ui.add_space(15.);

        ui.heading("Simulation parameters");
        ui.add(Label::new("Death probability"));
        ui.add(Slider::new(&mut self.death_prob, 0.0..=1.0));

        ui.add(Label::new("Infection probability"));
        ui.add(Slider::new(&mut self.infection_prob, 0.0..=1.0));

        ui.add(Label::new("Infection time (days)"));
        ui.add(Slider::new(&mut self.infection_time_s, 0.0..=30.0));
        ui.add_space(15.);

        ui.add(Label::new(format!(
            r#"Healthy: {} individuals
Infected: {} individuals
Recovered: {} individuals
Dead: {} individuals
Current time: {:.1} days"#,
            self.num_healthy,
            self.num_infected,
            self.num_recovered,
            self.num_dead,
            self.time_elapsed.as_secs_f32()
        )));
    }

    fn graph_ui(&mut self, ui: &mut Ui) {
        ui.vertical(|ui| {
            // Graph selector
            ComboBox::from_id_salt("graph_display")
                .selected_text(format!("{}", self.graph))
                .show_ui(ui, |ui| {
                    ui.selectable_value(
                        &mut self.graph,
                        GraphOptions::Healthy,
                        format!("{}", GraphOptions::Healthy),
                    );
                    ui.selectable_value(
                        &mut self.graph,
                        GraphOptions::Infected,
                        format!("{}", GraphOptions::Infected),
                    );
                    ui.selectable_value(
                        &mut self.graph,
                        GraphOptions::Recovered,
                        format!("{}", GraphOptions::Recovered),
                    );
                    ui.selectable_value(
                        &mut self.graph,
                        GraphOptions::Dead,
                        format!("{}", GraphOptions::Dead),
                    );
                });
            ui.add_space(20.);

            macro_rules! map_stats {
                ($field:ident) => {
                    self.stats
                        .iter()
                        .map(|stat| (stat.time, stat.$field))
                        .collect()
                };
            }

            let (times, stats): (Vec<Duration>, Vec<usize>) = match self.graph {
                GraphOptions::Healthy => map_stats!(num_healthy),
                GraphOptions::Infected => map_stats!(num_infected),
                GraphOptions::Recovered => map_stats!(num_recovered),
                GraphOptions::Dead => map_stats!(num_dead),
            };

            if let [.., max_time] = times[..] {
                let max_time = max_time.as_millis();
                let num_individuals =
                    self.num_healthy + self.num_infected + self.num_recovered + self.num_dead;

                let painter = ui.painter();
                let rect = ui.available_rect_before_wrap();
                let min = rect.min;
                let max = rect.max;

                let x_axis_text = painter.layout_no_wrap(
                    self.graph.to_string(),
                    FontId::default(),
                    Color32::GRAY,
                );
                let _x_axis = painter.add(TextShape::new(
                    Pos2 {
                        x: min.x + rect.width() / 2.0,
                        y: max.y,
                    },
                    x_axis_text.clone(),
                    Color32::GRAY,
                ));
                let y_axis_text =
                    painter.layout_no_wrap("time".to_owned(), FontId::default(), Color32::GRAY);
                let _y_axis = painter.add(
                    TextShape::new(
                        Pos2 {
                            x: min.x,
                            y: min.y + rect.height() / 2.0,
                        },
                        y_axis_text.clone(),
                        Color32::GRAY,
                    )
                    .with_angle(1.5 * PI),
                );

                let mut x_offset = min.x + y_axis_text.rect.max.x + 5.0;
                let mut y_offset = max.y + x_axis_text.rect.min.y - 5.0;
                let _x_axis = painter.add(Shape::LineSegment {
                    points: [
                        Pos2 {
                            x: x_offset,
                            y: y_offset,
                        },
                        Pos2 {
                            x: max.x,
                            y: y_offset,
                        },
                    ],
                    stroke: Stroke::new(1.0, Color32::GRAY),
                });
                let _y_axis = painter.add(Shape::LineSegment {
                    points: [
                        Pos2 {
                            x: x_offset,
                            y: min.y + 5.0,
                        },
                        Pos2 {
                            x: x_offset,
                            y: y_offset,
                        },
                    ],
                    stroke: Stroke::new(1.0, Color32::GRAY),
                });
                x_offset += 1.5;
                y_offset -= 1.5;
                let (w, h) = (max.x - x_offset - 4.0, y_offset - min.y - 4.0);

                let points = times.into_iter().zip(stats.into_iter()).map(|(t, s)| {
                    let x = t.as_millis() as f32 / max_time as f32;
                    let y = s as f32 / num_individuals as f32;
                    Shape::Circle(CircleShape {
                        center: Pos2 {
                            x: x_offset + x * w,
                            y: y_offset - y * h,
                        },
                        radius: 2.0,
                        fill: Color32::GRAY,
                        stroke: Stroke::NONE,
                    })
                });
                painter.extend(points);
            }
        });
    }

    fn step(&mut self) {
        // Amount of motion per ms
        const MOVE_AMOUNT: f32 = 0.01;

        let elapsed = self.last_frame_time.elapsed();
        let frame_time = elapsed.as_millis() as f32 * self.step_speed;
        self.time_elapsed += elapsed;
        self.last_frame_time = Instant::now();

        let infection_time = self.infection_time_s * 1000.0;
        let survival_prob = 1.0 - self.death_prob;
        let survive_this_frame = survival_prob.powf(frame_time / infection_time) as f64;
        let infection_prob = 1.0 - self.infection_prob;
        // Somewhat bastardized estimation
        let not_infected_this_frame = infection_prob.powf(frame_time / (1.5 / MOVE_AMOUNT)) as f64;

        let mut people_to_move = Vec::new();
        // Iterate over rows and cols
        for ((x_pos, y_pos), people) in self.grid.0.iter_mut() {
            // Step each individual
            let dist_to_move = MOVE_AMOUNT * frame_time;
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
                    let died = random_bool(1.0 - survive_this_frame);
                    if died {
                        person.state = InfectionState::Dead;
                        self.num_infected -= 1;
                        self.num_dead += 1;
                        return true;
                    }

                    // Update infection time
                    let new_infection_time = t + frame_time;
                    person.state = if new_infection_time > infection_time {
                        self.num_infected -= 1;
                        self.num_recovered += 1;
                        InfectionState::Recovered
                    } else {
                        InfectionState::Infected(new_infection_time)
                    };
                }

                // Do not retain if out of grid element
                let grid_x = pos.x as i32;
                let grid_y = pos.y as i32;
                grid_x != *x_pos || grid_y != *y_pos
            }));

            // Infection testing
            let contains_infected = people
                .iter()
                .any(|person| matches!(person.state, InfectionState::Infected(_)));
            if contains_infected {
                for person in people {
                    match (person.state, random_bool(1.0 - not_infected_this_frame)) {
                        (InfectionState::Healthy, true) => {
                            self.num_healthy -= 1;
                            self.num_infected += 1;
                            person.state = InfectionState::Infected(0.0)
                        }
                        _ => {}
                    }
                }
            }
        }
        // Move all people that need to be moved
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

        self.stats.push(PandemicSnapshot {
            time: self.time_elapsed,
            num_healthy: self.num_healthy,
            num_infected: self.num_infected,
            num_recovered: self.num_recovered,
            num_dead: self.num_dead,
        });
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
        const TARGET_RATIO: f32 = 16.0 / 10.0;
        let avail = ui.available_size() - Vec2 { x: 10.0, y: 10.0 };

        let aspect_ratio = avail.x / avail.y;

        let (x, y, x_off, y_off) = if aspect_ratio > TARGET_RATIO {
            // x is too large
            let target_x = avail.y * 1.6;
            let x_off = (avail.x - target_x) / 2.0;
            (target_x, avail.y, x_off, 0.0)
        } else if aspect_ratio > TARGET_RATIO {
            // y is too large
            let target_y = avail.x / 1.6;
            let y_off = (avail.y - target_y) / 2.0;
            (avail.x, target_y, 0.0, y_off)
        } else {
            (avail.x, avail.y, 0.0, 0.0)
        };
        let (x_ratio, y_ratio) = (x / X_MAX_FLOAT, y / Y_MAX_FLOAT);

        ui.painter().extend(self.0.values().flatten().map(|person| {
            Shape::Circle(CircleShape {
                center: Pos2 {
                    x: x_off + 5.0 + person.pos.x * x_ratio,
                    y: y_off + 5.0 + person.pos.y * y_ratio,
                },
                radius: 5.0,
                fill: match person.state {
                    InfectionState::Healthy => Color32::GREEN,
                    InfectionState::Infected(_) => Color32::RED,
                    InfectionState::Recovered => Color32::PURPLE,
                    InfectionState::Dead => {
                        unreachable!("Dead people should be removed before render!")
                    }
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

struct PandemicSnapshot {
    time: Duration,
    num_healthy: usize,
    num_infected: usize,
    num_recovered: usize,
    num_dead: usize,
}

#[derive(PartialEq)]
enum GraphOptions {
    Healthy,
    Infected,
    Recovered,
    Dead,
}
impl Display for GraphOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} Individuals",
            match self {
                Self::Healthy => "Healthy",
                Self::Infected => "Infected",
                Self::Recovered => "Recovered",
                Self::Dead => "Dead",
            }
        )
    }
}
