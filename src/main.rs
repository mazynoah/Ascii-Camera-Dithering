use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use image::{DynamicImage, GenericImageView, ImageBuffer, Rgb};
use nokhwa::{
    pixel_format::RgbFormat,
    utils::{CameraIndex, RequestedFormat, RequestedFormatType},
    Camera,
};
use std::{
    error::Error,
    io,
    time::{Duration, Instant},
};
use tui::{
    backend::{Backend, CrosstermBackend},
    layout::{Constraint, Direction, Layout, Rect},
    style::{Color, Style},
    text::Spans,
    widgets::{Block, Borders, List, ListItem, ListState, Paragraph, Wrap},
    Frame, Terminal,
};

struct StatefulList<T> {
    state: ListState,
    items: Vec<T>,
}

impl<T> StatefulList<T> {
    fn with_items(items: Vec<T>) -> StatefulList<T> {
        StatefulList {
            state: ListState::default(),
            items,
        }
    }

    fn next(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i >= self.items.len() - 1 {
                    0
                } else {
                    i + 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }

    fn selected(&self) -> Option<usize> {
        match self.state.selected() {
            Some(i) => return Some(i),
            None => return None,
        };
    }

    fn select_first(&mut self) {
        self.state.select(Some(0));
    }

    fn previous(&mut self) {
        let i = match self.state.selected() {
            Some(i) => {
                if i == 0 {
                    self.items.len() - 1
                } else {
                    i - 1
                }
            }
            None => 0,
        };
        self.state.select(Some(i));
    }
}

struct App {
    menu: StatefulList<(String, CameraIndex)>,
    camera: Option<Camera>,
    paused: bool,
    last_frame: Option<ImageBuffer<Rgb<u8>, Vec<u8>>>,
}

impl App {
    fn new() -> App {
        let cameras = match nokhwa::query(nokhwa::utils::ApiBackend::Auto) {
            Ok(cs) => cs,
            Err(_) => panic!("No camera found"),
        };

        let cameras: Vec<(String, CameraIndex)> = cameras
            .iter()
            .map(|c| (c.human_name(), c.index().clone()))
            .collect();

        App {
            menu: StatefulList::with_items(cameras),
            camera: None,
            paused: false,
            last_frame: None,
        }
    }
}

fn main() -> Result<(), Box<dyn Error>> {
    // setup terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // create app and run it
    let tick_rate = Duration::from_millis(15);
    let app = App::new();
    let res = run_app(&mut terminal, app, tick_rate);

    // restore terminal
    disable_raw_mode()?;
    execute!(
        terminal.backend_mut(),
        LeaveAlternateScreen,
        DisableMouseCapture,
    )?;
    terminal.show_cursor()?;

    if let Err(err) = res {
        println!("{:?}", err)
    }

    Ok(())
}

fn run_app<B: Backend>(
    terminal: &mut Terminal<B>,
    mut app: App,
    tick_rate: Duration,
) -> io::Result<()> {
    let mut last_tick = Instant::now();
    app.menu.select_first();
    loop {
        terminal.draw(|f| ui(f, &mut app))?;

        let timeout = tick_rate
            .checked_sub(last_tick.elapsed())
            .unwrap_or_else(|| Duration::from_secs(0));
        if crossterm::event::poll(timeout)? {
            if let Event::Key(key) = event::read()? {
                match app.camera.as_mut() {
                    Some(_) => match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Char(' ') => {
                            app.paused = !app.paused;
                            app.last_frame = None;
                        }
                        KeyCode::Esc => {
                            app.paused = false;
                            app.last_frame = None;
                            app.camera = None;
                        }
                        _ => {}
                    },
                    None => match key.code {
                        KeyCode::Char('q') => return Ok(()),
                        KeyCode::Down => app.menu.next(),
                        KeyCode::Up => app.menu.previous(),
                        KeyCode::Enter => {
                            let camera = &app.menu.items[app.menu.selected().unwrap()];

                            match Camera::new(
                                camera.1.clone(),
                                RequestedFormat::new::<RgbFormat>(
                                    RequestedFormatType::AbsoluteHighestFrameRate,
                                ),
                            ) {
                                Ok(cam) => app.camera = Some(cam),
                                Err(_) => {
                                    // todo: handle this error better (it's ugly)
                                    let index = app.menu.selected().unwrap();
                                    let x = &mut app.menu.items[index];
                                    x.0.push_str(" - An Error occured");
                                }
                            };
                        }

                        _ => {}
                    },
                }
            }
        }
        if last_tick.elapsed() >= tick_rate {
            last_tick = Instant::now();
        }
    }
}

const MENU: &str = r#"
Controls:
 - 'q' - quit the application
 - 'up' and 'down' arrow to navigate the camera list
 - 'enter' to select a camera
 - 'spacebar' to pause the viewer
 - 'esc' to return to the main menu

Known issues:
 - The framerate decreases when the window size or camera resolution increase 
 - The image is not very stable; lots of blinking and jittering
 - The image ratio is not maintained
 - The only way to scale up or down the viewer is either by resizing the terminal window or zooming
"#;

fn ui<B: Backend>(f: &mut Frame<B>, app: &mut App) {
    let size = f.size();

    match app.camera.as_mut() {
        None => {
            let chunks = Layout::default()
                .direction(Direction::Vertical)
                .constraints([Constraint::Percentage(25), Constraint::Percentage(75)].as_ref())
                .split(Rect::new(0, 0, size.width / 2, size.height));

            let cameras: Vec<ListItem> = app
                .menu
                .items
                .iter()
                .map(|i| {
                    let lines = vec![Spans::from(i.0.clone())];
                    ListItem::new(lines).style(Style::default().fg(Color::White))
                })
                .collect();

            // create a List from all the cameras and highlight the currently selected one
            let cameras = List::new(cameras)
                .block(Block::default().borders(Borders::ALL).title("Cameras"))
                .highlight_style(Style::default().bg(Color::White).fg(Color::Black))
                .highlight_symbol("> ");

            f.render_stateful_widget(cameras, chunks[0], &mut app.menu.state);

            let instructions = Paragraph::new(MENU)
                .block(Block::default().borders(Borders::ALL).title("Info"))
                .wrap(Wrap { trim: true });

            f.render_widget(instructions, chunks[1]);
        }
        Some(camera) => {
            let mut title = "View";

            let dithered_text = match app.last_frame.as_mut() {
                Some(img) => {
                    title = "View - Paused";

                    // rezise the image
                    let image = DynamicImage::from(img.clone()).resize_exact(
                        size.width.into(),
                        size.height.into(),
                        image::imageops::FilterType::Nearest,
                    );

                    dither_image(image)
                }
                None => {
                    // get a new frame
                    let frame = camera.frame().unwrap();
                    let decoded = frame.decode_image::<RgbFormat>().unwrap();

                    if app.paused {
                        app.last_frame = Some(decoded.clone());
                    }

                    // rezise the image
                    // ! This does not keep aspect ratio
                    let image = DynamicImage::from(decoded).resize_exact(
                        size.width.into(),
                        size.height.into(),
                        image::imageops::FilterType::Nearest,
                    );

                    dither_image(image)
                }
            };

            let paragraph = Paragraph::new(dithered_text)
                .block(Block::default().borders(Borders::ALL).title(title));

            f.render_widget(paragraph, size);
        }
    }
}

const ASCII_CHARS: &str = " .:-=+*#%@";

fn dither_image(image: DynamicImage) -> String {
    let (width, height) = image.dimensions();

    let binding = image.grayscale();
    let image = match binding.as_luma8() {
        Some(img) => img,
        None => panic!("Image error"),
    };

    let mut ascii_image: Vec<Vec<u8>> = vec![vec![0; width as usize]; height as usize];

    // normalize the image to the range [0, 1]
    let min = image.iter().min().unwrap();
    let max = image.iter().max().unwrap();
    let norm_image = ImageBuffer::from_fn(width, height, |x, y| {
        let pixel = image.get_pixel(x, y);
        let value = (pixel[0] as f32 - *min as f32) / (max - min) as f32;
        image::Luma([(value * 255.0) as u8])
    });

    // scale the image to the range of ASCII characters
    let scale_image = ImageBuffer::from_fn(width, height, |x, y| {
        let pixel = norm_image.get_pixel(x, y);
        let value = (pixel[0] as f32 / 255.0 * (ASCII_CHARS.len() - 1) as f32).round() as u8;
        image::Luma([value])
    });

    // replace the pixel values with their corresponding ASCII characters
    for (x, y, pixel) in scale_image.enumerate_pixels() {
        let value = pixel[0];
        let ascii_char = ASCII_CHARS.chars().nth(value as usize).unwrap();
        ascii_image[y as usize][x as usize] = ascii_char as u8;
    }

    // save and return the resulting ascii art
    let mut output = String::new();
    for row in ascii_image {
        let mut row_string: String = row.iter().map(|c| *c as char).collect();
        row_string.push('\n');
        output.push_str(&row_string);
    }
    output
}
