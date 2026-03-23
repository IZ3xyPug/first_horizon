use quartz::*;
use ramp::prism;

// ── Virtual resolution ────────────────────────────────────────────────────────
const VW: f32 = 3840.0;
const VH: f32 = 2160.0;

// ── World ─────────────────────────────────────────────────────────────────────
const WORLD_SIZE: f32 = 80_000.0;

// ── Ship ──────────────────────────────────────────────────────────────────────
const SHIP_W: f32 = 80.0;
const SHIP_H: f32 = 80.0;
const THRUST_FORCE: f32 = 0.45;
const ROTATION_SPEED: f32 = 3.5; // degrees per tick
const MAX_SPEED: f32 = 18.0;
const SAFE_LAND_SPEED: f32 = 4.5;

// ── Resource drains (per tick) ────────────────────────────────────────────────
const FUEL_DRAIN_THRUST: f32 = 0.012;
const FUEL_DRAIN_PASSIVE: f32 = 0.002;
const OXYGEN_DRAIN: f32 = 0.008;
const HULL_IMPACT_DAMAGE: f32 = 18.0;

// ── World generation ──────────────────────────────────────────────────────────
const PLANET_COUNT: usize = 18;
const MIN_PLANET_R: f32 = 120.0;
const MAX_PLANET_R: f32 = 340.0;
const MIN_PLANET_DIST: f32 = 3000.0;
const ASTEROID_COUNT: usize = 35;
const ASTEROID_MIN_SIZE: f32 = 35.0;
const ASTEROID_MAX_SIZE: f32 = 110.0;
const STAR_COUNT: usize = 200;

const PLANET_COLORS: [(u8, u8, u8); 8] = [
    (80, 160, 255),
    (255, 120, 60),
    (120, 220, 120),
    (220, 200, 80),
    (180, 80, 220),
    (80, 220, 200),
    (255, 160, 180),
    (160, 200, 255),
];

// ─────────────────────────────────────────────────────────────────────────────
// Deterministic LCG pseudo-random
// ─────────────────────────────────────────────────────────────────────────────
fn lcg(seed: &mut u64) -> f32 {
    *seed = seed.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
    ((*seed >> 33) as f32) / (u32::MAX as f32)
}

fn lcg_range(seed: &mut u64, lo: f32, hi: f32) -> f32 {
    lo + lcg(seed) * (hi - lo)
}

// ─────────────────────────────────────────────────────────────────────────────
// Image helpers
// ─────────────────────────────────────────────────────────────────────────────
fn solid(r: u8, g: u8, b: u8, a: u8) -> image::RgbaImage {
    let mut img = image::RgbaImage::new(1, 1);
    img.put_pixel(0, 0, image::Rgba([r, g, b, a]));
    img
}

fn planet_image(radius: u32, r: u8, g: u8, b: u8) -> image::RgbaImage {
    let d = radius * 2;
    let mut img = image::RgbaImage::new(d, d);
    let cx = radius as f32;
    let rf = radius as f32;
    for py in 0..d {
        for px in 0..d {
            let dx = px as f32 - cx + 0.5;
            let dy = py as f32 - cx + 0.5;
            let dist = (dx*dx + dy*dy).sqrt();
            if dist <= rf {
                let rim = ((rf - dist) / rf).min(1.0);
                img.put_pixel(px, py, image::Rgba([
                    (r as f32 * (0.7 + 0.3*rim)).min(255.0) as u8,
                    (g as f32 * (0.7 + 0.3*rim)).min(255.0) as u8,
                    (b as f32 * (0.7 + 0.3*rim)).min(255.0) as u8,
                    255,
                ]));
            }
        }
    }
    img
}

fn asteroid_image(size: u32, seed: u64) -> image::RgbaImage {
    let mut s = seed;
    let mut img = image::RgbaImage::new(size, size);
    let cx = size as f32 / 2.0;
    let base_r = (size as f32 * 0.38).max(4.0);
    let steps = 12usize;
    let radii: Vec<f32> = (0..steps).map(|_| base_r * lcg_range(&mut s, 0.6, 1.0)).collect();
    for py in 0..size {
        for px in 0..size {
            let dx = px as f32 - cx;
            let dy = py as f32 - cx;
            let angle = dy.atan2(dx);
            let norm = (angle + std::f32::consts::PI) / (2.0 * std::f32::consts::PI);
            let idx  = (norm * steps as f32) as usize % steps;
            let next = (idx + 1) % steps;
            let r = radii[idx] * (1.0 - (norm*steps as f32).fract()) + radii[next] * (norm*steps as f32).fract();
            if (dx*dx + dy*dy).sqrt() <= r {
                img.put_pixel(px, py, image::Rgba([160, 140, 120, 255]));
            }
        }
    }
    img
}

fn ship_image(w: u32, h: u32) -> image::RgbaImage {
    let mut img = image::RgbaImage::new(w, h);
    let cx = w as f32 / 2.0;
    for py in 0..h {
        for px in 0..w {
            let t = py as f32 / h as f32;
            if (px as f32 - cx).abs() < cx * t {
                let (r, g, b) = if t > 0.80 { (255, 160, 60) } else { (200, 220, 255) };
                img.put_pixel(px, py, image::Rgba([r, g, b, 255]));
            }
        }
    }
    img
}

/// Nearest-neighbour rotation around the image centre.
fn rotate_image(src: &image::RgbaImage, angle_rad: f32) -> image::RgbaImage {
    let (w, h) = src.dimensions();
    let mut dst = image::RgbaImage::new(w, h);
    let cx = w as f32 / 2.0;
    let cy = h as f32 / 2.0;
    let (cos_a, sin_a) = (angle_rad.cos(), angle_rad.sin());
    for dy in 0..h {
        for dx in 0..w {
            let rx = dx as f32 - cx;
            let ry = dy as f32 - cy;
            let sx = (cos_a * rx + sin_a * ry + cx) as i32;
            let sy = (-sin_a * rx + cos_a * ry + cy) as i32;
            if sx >= 0 && sx < w as i32 && sy >= 0 && sy < h as i32 {
                dst.put_pixel(dx, dy, *src.get_pixel(sx as u32, sy as u32));
            }
        }
    }
    dst
}

fn bar_image(w: u32, h: u32, fill: f32, r: u8, g: u8, b: u8) -> image::RgbaImage {
    let mut img = image::RgbaImage::new(w, h);
    let filled = (w as f32 * fill.clamp(0.0, 1.0)) as u32;
    for py in 0..h {
        for px in 0..w {
            let pixel = if px == 0 || px == w-1 || py == 0 || py == h-1 {
                image::Rgba([200, 200, 200, 255])
            } else if px < filled {
                image::Rgba([r, g, b, 255])
            } else {
                image::Rgba([30, 30, 40, 220])
            };
            img.put_pixel(px, py, pixel);
        }
    }
    img
}

fn bar_to_image(w: u32, h: u32, fill: f32, r: u8, g: u8, b: u8) -> Image {
    Image {
        shape: ShapeType::Rectangle(0.0, (w as f32, h as f32), 0.0),
        image: bar_image(w, h, fill, r, g, b).into(),
        color: None,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// World generation
// ─────────────────────────────────────────────────────────────────────────────
struct PlanetSpec   { x: f32, y: f32, radius: f32, color_idx: usize }
struct AsteroidSpec { x: f32, y: f32, size: f32, vx: f32, vy: f32, seed: u64 }

fn generate_planets(seed: &mut u64) -> Vec<PlanetSpec> {
    let mut planets = vec![PlanetSpec {
        x: WORLD_SIZE/2.0 + 1200.0, y: WORLD_SIZE/2.0 - 400.0,
        radius: 220.0, color_idx: 2,
    }];
    let mut attempts = 0u32;
    while planets.len() < PLANET_COUNT && attempts < 5000 {
        attempts += 1;
        let x = lcg_range(seed, 1000.0, WORLD_SIZE - 1000.0);
        let y = lcg_range(seed, 1000.0, WORLD_SIZE - 1000.0);
        let r = lcg_range(seed, MIN_PLANET_R, MAX_PLANET_R);
        let col = (lcg(seed) * 8.0) as usize % 8;
        let too_close  = planets.iter().any(|p| { let dx=p.x-x; let dy=p.y-y; (dx*dx+dy*dy).sqrt() < MIN_PLANET_DIST });
        let sdx = WORLD_SIZE/2.0 - x; let sdy = WORLD_SIZE/2.0 - y;
        let near_spawn = (sdx*sdx+sdy*sdy).sqrt() < 800.0;
        if !too_close && !near_spawn { planets.push(PlanetSpec { x, y, radius: r, color_idx: col }); }
    }
    planets
}

fn generate_asteroids(seed: &mut u64, planets: &[PlanetSpec]) -> Vec<AsteroidSpec> {
    let mut asteroids = Vec::new();
    let mut attempts = 0u32;
    while asteroids.len() < ASTEROID_COUNT && attempts < 2000 {
        attempts += 1;
        let x  = lcg_range(seed, 500.0, WORLD_SIZE-500.0);
        let y  = lcg_range(seed, 500.0, WORLD_SIZE-500.0);
        let sz = lcg_range(seed, ASTEROID_MIN_SIZE, ASTEROID_MAX_SIZE);
        let vx = lcg_range(seed, -1.2, 1.2);
        let vy = lcg_range(seed, -1.2, 1.2);
        let aseed = *seed ^ 0xDEAD_BEEF;
        let near_planet = planets.iter().any(|p| { let dx=p.x-x; let dy=p.y-y; (dx*dx+dy*dy).sqrt() < p.radius+400.0 });
        let sdx = WORLD_SIZE/2.0-x; let sdy = WORLD_SIZE/2.0-y;
        let near_spawn  = (sdx*sdx+sdy*sdy).sqrt() < 1200.0;
        if !near_planet && !near_spawn { asteroids.push(AsteroidSpec { x, y, size: sz, vx, vy, seed: aseed }); }
    }
    asteroids
}

// ─────────────────────────────────────────────────────────────────────────────
// Menu scene
// ─────────────────────────────────────────────────────────────────────────────
fn build_menu_scene(ctx: &mut Context) -> Scene {
    let bg = GameObject::new_rect(
        ctx, "menu_bg".into(),
        Some(Image { shape: ShapeType::Rectangle(0.0, (VW, VH), 0.0), image: solid(5,5,20,255).into(), color: None }),
        (VW, VH), (0.0, 0.0), vec![], (0.0, 0.0), (1.0, 1.0), 0.0,
    );

    let title = {
        let (w, h) = (1400u32, 220u32);
        let mut img = image::RgbaImage::new(w, h);
        for py in 0..h { for px in 0..w {
            let t = px as f32 / w as f32;
            img.put_pixel(px, py, image::Rgba([(100.0+155.0*t) as u8, (200.0+55.0*t) as u8, 255, 255]));
        }}
        GameObject::new_rect(ctx, "menu_title".into(),
            Some(Image { shape: ShapeType::Rectangle(0.0, (w as f32, h as f32), 0.0), image: img.into(), color: None }),
            (w as f32, h as f32), (VW/2.0 - w as f32/2.0, VH*0.28), vec!["ui".into()], (0.0,0.0), (1.0,1.0), 0.0)
    };

    let subtitle = {
        let (w, h) = (900u32, 80u32);
        let mut img = image::RgbaImage::new(w, h);
        for py in 0..h { for px in 0..w { img.put_pixel(px, py, image::Rgba([160,200,255,220])); }}
        GameObject::new_rect(ctx, "menu_sub".into(),
            Some(Image { shape: ShapeType::Rectangle(0.0, (w as f32, h as f32), 0.0), image: img.into(), color: None }),
            (w as f32, h as f32), (VW/2.0 - w as f32/2.0, VH*0.48), vec!["ui".into()], (0.0,0.0), (1.0,1.0), 0.0)
    };

    let start_btn = {
        let (w, h) = (500u32, 120u32);
        let mut img = image::RgbaImage::new(w, h);
        for py in 0..h { for px in 0..w {
            let border = px==0||px==w-1||py==0||py==h-1||px==1||px==w-2||py==1||py==h-2;
            img.put_pixel(px, py, image::Rgba([60, if border {120} else {40}, 180, 240]));
        }}
        GameObject::new_rect(ctx, "start_btn".into(),
            Some(Image { shape: ShapeType::Rectangle(8.0, (w as f32, h as f32), 0.0), image: img.into(), color: None }),
            (w as f32, h as f32), (VW/2.0 - w as f32/2.0, VH*0.62),
            vec!["ui".into(), "button".into()], (0.0,0.0), (1.0,1.0), 0.0)
    };

    let mut scene = Scene::new("menu")
        .with_object("menu_bg",   bg)
        .with_object("menu_title", title)
        .with_object("menu_sub",  subtitle)
        .with_object("start_btn", start_btn);

    let mut seed: u64 = 0xCAFEBABE;
    for i in 0..80usize {
        let x  = lcg_range(&mut seed, 0.0, VW);
        let y  = lcg_range(&mut seed, 0.0, VH);
        let br = lcg_range(&mut seed, 80.0, 255.0) as u8;
        let sz = if lcg(&mut seed) > 0.85 { 6.0 } else { 3.0 };
        let id = format!("mstar_{i}");
        let obj = GameObject::new_rect(ctx, id.clone().into(),
            Some(Image { shape: ShapeType::Rectangle(0.0, (sz,sz), 0.0), image: solid(br,br,br,255).into(), color: None }),
            (sz,sz), (x,y), vec![], (0.0,0.0), (1.0,1.0), 0.0);
        scene = scene.with_object(id, obj);
    }

    scene
        .with_event(
            GameEvent::KeyPress { key: Key::Named(NamedKey::Space), action: Action::Custom { name: "goto_game".into() }, target: Target::name("start_btn") },
            Target::name("start_btn"),
        )
        .with_event(
            GameEvent::MousePress { action: Action::Custom { name: "goto_game".into() }, target: Target::name("start_btn"), button: Some(MouseButton::Left) },
            Target::name("start_btn"),
        )
        .on_enter(|canvas| {
            canvas.register_custom_event("goto_game".into(), |c| c.load_scene("game"));
        })
}

// ─────────────────────────────────────────────────────────────────────────────
// Game Over scene
// ─────────────────────────────────────────────────────────────────────────────
fn build_gameover_scene(ctx: &mut Context) -> Scene {
    let bg = GameObject::new_rect(ctx, "go_bg".into(),
        Some(Image { shape: ShapeType::Rectangle(0.0, (VW,VH), 0.0), image: solid(10,0,0,255).into(), color: None }),
        (VW,VH), (0.0,0.0), vec![], (0.0,0.0), (1.0,1.0), 0.0);

    let title = {
        let (w, h) = (1200u32, 200u32);
        let mut img = image::RgbaImage::new(w, h);
        for py in 0..h { for px in 0..w {
            img.put_pixel(px, py, image::Rgba([255, (60.0*(1.0-py as f32/h as f32)) as u8, 40, 255]));
        }}
        GameObject::new_rect(ctx, "go_title".into(),
            Some(Image { shape: ShapeType::Rectangle(0.0, (w as f32, h as f32), 0.0), image: img.into(), color: None }),
            (w as f32, h as f32), (VW/2.0 - w as f32/2.0, VH*0.25), vec!["ui".into()], (0.0,0.0), (1.0,1.0), 0.0)
    };

    let dist_label = {
        let (w, h) = (700u32, 90u32);
        let mut img = image::RgbaImage::new(w, h);
        for py in 0..h { for px in 0..w { img.put_pixel(px, py, image::Rgba([200,200,255,200])); }}
        GameObject::new_rect(ctx, "go_dist".into(),
            Some(Image { shape: ShapeType::Rectangle(0.0, (w as f32, h as f32), 0.0), image: img.into(), color: None }),
            (w as f32, h as f32), (VW/2.0 - w as f32/2.0, VH*0.46), vec!["ui".into()], (0.0,0.0), (1.0,1.0), 0.0)
    };

    // Helper closure to build a button
    let make_btn = |ctx: &mut Context, id: &str, (br, bg_c, bb): (u8,u8,u8), y: f32| {
        let (w, h) = (480u32, 120u32);
        let mut img = image::RgbaImage::new(w, h);
        for py in 0..h { for px in 0..w {
            let border = px==0||px==w-1||py==0||py==h-1;
            let c = if border {120u8} else {40u8};
            img.put_pixel(px, py, image::Rgba([br.saturating_add(c/2), bg_c.saturating_add(c/2), bb, 240]));
        }}
        GameObject::new_rect(ctx, id.to_string().into(),
            Some(Image { shape: ShapeType::Rectangle(8.0, (w as f32, h as f32), 0.0), image: img.into(), color: None }),
            (w as f32, h as f32), (VW/2.0 - w as f32/2.0, y),
            vec!["ui".into(), "button".into()], (0.0,0.0), (1.0,1.0), 0.0)
    };

    let retry_btn = make_btn(ctx, "retry_btn", (60, 40, 180), VH*0.60);
    let menu_btn  = make_btn(ctx, "menu_btn",  (20, 80, 160), VH*0.74);

    Scene::new("gameover")
        .with_object("go_bg",      bg)
        .with_object("go_title",   title)
        .with_object("go_dist",    dist_label)
        .with_object("retry_btn",  retry_btn)
        .with_object("menu_btn",   menu_btn)
        .with_event(
            GameEvent::MousePress { action: Action::Custom { name: "go_retry".into() }, target: Target::name("retry_btn"), button: Some(MouseButton::Left) },
            Target::name("retry_btn"),
        )
        .with_event(
            GameEvent::KeyPress { key: Key::Named(NamedKey::Space), action: Action::Custom { name: "go_retry".into() }, target: Target::name("retry_btn") },
            Target::name("retry_btn"),
        )
        .with_event(
            GameEvent::MousePress { action: Action::Custom { name: "go_menu".into() }, target: Target::name("menu_btn"), button: Some(MouseButton::Left) },
            Target::name("menu_btn"),
        )
        .on_enter(|canvas| {
            canvas.register_custom_event("go_retry".into(), |c| c.load_scene("game"));
            canvas.register_custom_event("go_menu".into(),  |c| c.load_scene("menu"));
        })
}

// ─────────────────────────────────────────────────────────────────────────────
// Game scene
// ─────────────────────────────────────────────────────────────────────────────
fn build_game_scene(ctx: &mut Context) -> Scene {
    let mut seed: u64 = 0x1234_5678_9ABC_DEF0;

    // Space background
    let space_bg = GameObject::new_rect(ctx, "space_bg".into(),
        Some(Image { shape: ShapeType::Rectangle(0.0, (WORLD_SIZE,WORLD_SIZE), 0.0), image: solid(4,4,14,255).into(), color: None }),
        (WORLD_SIZE,WORLD_SIZE), (0.0,0.0), vec![], (0.0,0.0), (1.0,1.0), 0.0);

    let mut scene = Scene::new("game").with_object("space_bg", space_bg);

    // Stars
    for i in 0..STAR_COUNT {
        let x  = lcg_range(&mut seed, 100.0, WORLD_SIZE-100.0);
        let y  = lcg_range(&mut seed, 100.0, WORLD_SIZE-100.0);
        let br = lcg_range(&mut seed, 80.0, 255.0) as u8;
        let sz = if lcg(&mut seed) > 0.88 { 6.0 } else { 3.0 };
        let id = format!("star_{i}");
        let obj = GameObject::new_rect(ctx, id.clone().into(),
            Some(Image { shape: ShapeType::Rectangle(0.0, (sz,sz), 0.0), image: solid(br, br, br.saturating_add(20), 255).into(), color: None }),
            (sz,sz), (x,y), vec!["star".into()], (0.0,0.0), (1.0,1.0), 0.0);
        scene = scene.with_object(id, obj);
    }

    // Planets — as_platform() so the engine's built-in collision handles landing
    let planets = generate_planets(&mut seed);
    for (i, p) in planets.iter().enumerate() {
        let (r, g, b) = PLANET_COLORS[p.color_idx];
        let id = format!("planet_{i}");
        let obj = GameObject::new_rect(ctx, id.clone().into(),
            Some(Image {
                shape: ShapeType::Rectangle(p.radius, (p.radius*2.0, p.radius*2.0), 0.0),
                image: planet_image(p.radius as u32, r, g, b).into(),
                color: None,
            }),
            (p.radius*2.0, p.radius*2.0), (p.x-p.radius, p.y-p.radius),
            vec!["planet".into()], (0.0,0.0), (1.0,1.0), 0.0,
        ).as_platform();
        scene = scene.with_object(id, obj);
    }

    // Asteroids — given initial momentum, resistance=1 so they drift forever
    let asteroids = generate_asteroids(&mut seed, &planets);
    for (i, a) in asteroids.iter().enumerate() {
        let id = format!("asteroid_{i}");
        let obj = GameObject::new_rect(ctx, id.clone().into(),
            Some(Image {
                shape: ShapeType::Rectangle(0.0, (a.size, a.size), 0.0),
                image: asteroid_image(a.size as u32, a.seed).into(),
                color: None,
            }),
            (a.size, a.size), (a.x, a.y),
            vec!["asteroid".into()], (a.vx, a.vy), (1.0,1.0), 0.0,
        );
        scene = scene.with_object(id, obj);
    }

    // Player ship
    let spawn = (WORLD_SIZE/2.0 - SHIP_W/2.0, WORLD_SIZE/2.0 - SHIP_H/2.0);
    let player = GameObject::new_rect(ctx, "player".into(),
        Some(Image { shape: ShapeType::Rectangle(0.0, (SHIP_W,SHIP_H), 0.0), image: ship_image(SHIP_W as u32, SHIP_H as u32).into(), color: None }),
        (SHIP_W,SHIP_H), spawn, vec!["player".into()], (0.0,0.0), (1.0,1.0), 0.0);
    scene = scene.with_object("player", player);

    // Thrust flame (hidden until thrust fires)
    let mut flame = GameObject::new_rect(ctx, "flame".into(),
        Some(Image { shape: ShapeType::Rectangle(0.0, (SHIP_W*0.4, SHIP_H*0.5), 0.0), image: solid(255,140,40,200).into(), color: None }),
        (SHIP_W*0.4, SHIP_H*0.5), (spawn.0+SHIP_W*0.3, spawn.1+SHIP_H),
        vec!["flame".into()], (0.0,0.0), (1.0,1.0), 0.0);
    flame.visible = false;
    scene = scene.with_object("flame", flame);

    // Landing proximity ring
    let land_ring = {
        let sz = 180u32;
        let mut img = image::RgbaImage::new(sz, sz);
        let cx = sz as f32 / 2.0;
        for py in 0..sz { for px in 0..sz {
            let d = ((px as f32-cx).powi(2) + (py as f32-cx).powi(2)).sqrt();
            if d >= cx-6.0 && d <= cx { img.put_pixel(px, py, image::Rgba([80,255,120,180])); }
        }}
        let mut obj = GameObject::new_rect(ctx, "land_ring".into(),
            Some(Image { shape: ShapeType::Rectangle(0.0, (sz as f32, sz as f32), 0.0), image: img.into(), color: None }),
            (sz as f32, sz as f32), (spawn.0-50.0, spawn.1-50.0), vec![], (0.0,0.0), (1.0,1.0), 0.0);
        obj.visible = false;
        obj
    };
    scene = scene.with_object("land_ring", land_ring);

    // HUD bars
    scene = scene
        .with_object("fuel_bar", GameObject::new_rect(ctx, "fuel_bar".into(), Some(bar_to_image(400,36,1.0,80,200,255)),  (400.0,36.0), (60.0,60.0),  vec!["hud".into()], (0.0,0.0), (1.0,1.0), 0.0))
        .with_object("oxy_bar",  GameObject::new_rect(ctx, "oxy_bar".into(),  Some(bar_to_image(400,36,1.0,80,255,160)),  (400.0,36.0), (60.0,118.0), vec!["hud".into()], (0.0,0.0), (1.0,1.0), 0.0))
        .with_object("hull_bar", GameObject::new_rect(ctx, "hull_bar".into(), Some(bar_to_image(400,36,1.0,255,120,80)),  (400.0,36.0), (60.0,176.0), vec!["hud".into()], (0.0,0.0), (1.0,1.0), 0.0));

    // Colour labels beside bars
    let make_label = |ctx: &mut Context, id: &str, r:u8, g:u8, b:u8, y:f32| {
        GameObject::new_rect(ctx, id.to_string().into(),
            Some(Image { shape: ShapeType::Rectangle(0.0,(40.0,32.0),0.0), image: solid(r,g,b,255).into(), color: None }),
            (40.0,32.0), (14.0,y), vec!["hud".into()], (0.0,0.0), (1.0,1.0), 0.0)
    };
    scene = scene
        .with_object("fuel_lbl", make_label(ctx,"fuel_lbl",80,200,255,64.0))
        .with_object("oxy_lbl",  make_label(ctx,"oxy_lbl", 80,255,160,122.0))
        .with_object("hull_lbl", make_label(ctx,"hull_lbl",255,120,80,180.0));

    // "LANDED" flash
    let mut landed_flash = {
        let (w,h) = (600u32,100u32);
        let mut img = image::RgbaImage::new(w,h);
        for py in 0..h { for px in 0..w { img.put_pixel(px,py,image::Rgba([80,255,120,230])); }}
        GameObject::new_rect(ctx, "landed_flash".into(),
            Some(Image { shape: ShapeType::Rectangle(0.0,(w as f32,h as f32),0.0), image: img.into(), color: None }),
            (w as f32,h as f32), (VW/2.0-w as f32/2.0, VH*0.42), vec!["hud".into()], (0.0,0.0), (1.0,1.0), 0.0)
    };
    landed_flash.visible = false;
    scene = scene.with_object("landed_flash", landed_flash);

    // "LOW FUEL" warning
    let mut low_fuel_warn = {
        let (w,h) = (500u32,80u32);
        let mut img = image::RgbaImage::new(w,h);
        for py in 0..h { for px in 0..w { img.put_pixel(px,py,image::Rgba([255,200,40,230])); }}
        GameObject::new_rect(ctx, "low_fuel_warn".into(),
            Some(Image { shape: ShapeType::Rectangle(0.0,(w as f32,h as f32),0.0), image: img.into(), color: None }),
            (w as f32,h as f32), (VW/2.0-w as f32/2.0, VH*0.08), vec!["hud".into()], (0.0,0.0), (1.0,1.0), 0.0)
    };
    low_fuel_warn.visible = false;
    scene = scene.with_object("low_fuel_warn", low_fuel_warn);

    // ── Key bindings ──────────────────────────────────────────────────────────
    for key in [Key::Named(NamedKey::ArrowUp), Key::Character("w".into())] {
        scene = scene.with_event(
            GameEvent::KeyHold { key, action: Action::Custom { name: "thrust".into() }, target: Target::name("player") },
            Target::name("player"),
        );
    }
    for key in [Key::Named(NamedKey::ArrowLeft), Key::Character("a".into())] {
        scene = scene.with_event(
            GameEvent::KeyHold { key, action: Action::Custom { name: "rotate_left".into() }, target: Target::name("player") },
            Target::name("player"),
        );
    }
    for key in [Key::Named(NamedKey::ArrowRight), Key::Character("d".into())] {
        scene = scene.with_event(
            GameEvent::KeyHold { key, action: Action::Custom { name: "rotate_right".into() }, target: Target::name("player") },
            Target::name("player"),
        );
    }
    for key in [Key::Named(NamedKey::ArrowDown), Key::Character("s".into())] {
        scene = scene.with_event(
            GameEvent::KeyHold { key, action: Action::Custom { name: "brake".into() }, target: Target::name("player") },
            Target::name("player"),
        );
    }

    // Asteroid collision — engine fires this via built-in AABB detection
    scene = scene.with_event(
        GameEvent::Collision { action: Action::Custom { name: "asteroid_hit".into() }, target: Target::tag("asteroid") },
        Target::name("player"),
    );

    // ── on_enter ──────────────────────────────────────────────────────────────
    scene.on_enter(|canvas| {
        let mut cam = Camera::new((WORLD_SIZE, WORLD_SIZE), (VW, VH));
        cam.follow(Some(Target::name("player")));
        cam.lerp_speed = 0.12;
        canvas.set_camera(cam);

        use std::sync::{Arc, Mutex};

        #[derive(Clone)]
        struct State {
            angle_deg:        f32,
            last_drawn_angle: f32,
            fuel:             f32,
            oxygen:           f32,
            hull:             f32,
            landed_on:        bool,
            landed_ticks:     u32,
            distance:         f32,
            spawn:            (f32, f32),
            dead:             bool,
            warn_tick:        u32,
        }

        let base_ship_img = ship_image(SHIP_W as u32, SHIP_H as u32);

        let state = Arc::new(Mutex::new(State {
            angle_deg:        0.0,
            last_drawn_angle: f32::NAN,
            fuel:             100.0,
            oxygen:           100.0,
            hull:             100.0,
            landed_on:        false,
            landed_ticks:     0,
            distance:         0.0,
            spawn:            (WORLD_SIZE/2.0, WORLD_SIZE/2.0),
            dead:             false,
            warn_tick:        0,
        }));

        // ── Rotate left ───────────────────────────────────────────────────────
        let st = state.clone();
        canvas.register_custom_event("rotate_left".into(), move |_c| {
            let mut s = st.lock().unwrap();
            if !s.landed_on { s.angle_deg -= ROTATION_SPEED; }
        });

        // ── Rotate right ──────────────────────────────────────────────────────
        let st = state.clone();
        canvas.register_custom_event("rotate_right".into(), move |_c| {
            let mut s = st.lock().unwrap();
            if !s.landed_on { s.angle_deg += ROTATION_SPEED; }
        });

        // ── Thrust ────────────────────────────────────────────────────────────
        let st = state.clone();
        canvas.register_custom_event("thrust".into(), move |c| {
            let mut s = st.lock().unwrap();
            if s.landed_on || s.fuel <= 0.0 { return; }
            let angle_rad = s.angle_deg.to_radians();
            let ax = angle_rad.sin() * THRUST_FORCE;
            let ay = -angle_rad.cos() * THRUST_FORCE;
            s.fuel -= FUEL_DRAIN_THRUST;
            drop(s);
            // Clamp speed manually since ApplyMomentum doesn't clamp
            if let Some(obj) = c.get_game_object_mut("player") {
                obj.momentum.0 = (obj.momentum.0 + ax).clamp(-MAX_SPEED, MAX_SPEED);
                obj.momentum.1 = (obj.momentum.1 + ay).clamp(-MAX_SPEED, MAX_SPEED);
            }
            // Show flame via the API
            c.run(Action::Show { target: Target::name("flame") });
        });

        // ── Brake — SetMomentum to 90% of current ─────────────────────────────
        let st = state.clone();
        canvas.register_custom_event("brake".into(), move |c| {
            let s = st.lock().unwrap();
            if s.landed_on { return; }
            drop(s);
            if let Some(obj) = c.get_game_object("player") {
                let (mx, my) = (obj.momentum.0 * 0.90, obj.momentum.1 * 0.90);
                c.run(Action::SetMomentum { target: Target::name("player"), value: (mx, my) });
            }
        });

        // ── Asteroid hit — confirmed via collision_between ────────────────────
        let st = state.clone();
        canvas.register_custom_event("asteroid_hit".into(), move |c| {
            let mut s = st.lock().unwrap();
            if s.landed_on { return; }
            // Double-check with the API to avoid phantom fires
            if !c.collision_between(&Target::name("player"), &Target::tag("asteroid")) { return; }
            s.hull -= HULL_IMPACT_DAMAGE * 0.5;
            drop(s);
            // Bounce using SetMomentum
            if let Some(obj) = c.get_game_object("player") {
                let (mx, my) = (
                    (obj.momentum.0 * -1.2).clamp(-MAX_SPEED, MAX_SPEED),
                    (obj.momentum.1 * -1.2).clamp(-MAX_SPEED, MAX_SPEED),
                );
                c.run(Action::SetMomentum { target: Target::name("player"), value: (mx, my) });
            }
        });

        // ── Main tick ─────────────────────────────────────────────────────────
        let st = state.clone();
        canvas.on_update(move |c| {
            // Redraw rotated ship sprite only when the angle changes
            {
                let mut s = st.lock().unwrap();
                if (s.angle_deg - s.last_drawn_angle).abs() > 0.01 {
                    s.last_drawn_angle = s.angle_deg;
                    let rotated = rotate_image(&base_ship_img, s.angle_deg.to_radians());
                    drop(s);
                    if let Some(obj) = c.get_game_object_mut("player") {
                        obj.set_image(Image {
                            shape: ShapeType::Rectangle(0.0, (SHIP_W, SHIP_H), 0.0),
                            image: rotated.into(),
                            color: None,
                        });
                    }
                }
            }

            let mut s = st.lock().unwrap();
            if s.dead { return; }

            // Hide flame every tick; thrust handler re-shows it when active
            c.run(Action::Hide { target: Target::name("flame") });

            // Read player position + velocity
            let (px, py, vx, vy) = c.get_game_object("player")
                .map(|p| (p.position.0, p.position.1, p.momentum.0, p.momentum.1))
                .unwrap_or((s.spawn.0, s.spawn.1, 0.0, 0.0));
            let speed = (vx*vx + vy*vy).sqrt();

            // Track max distance from spawn
            let (dx, dy) = (px - s.spawn.0, py - s.spawn.1);
            s.distance = s.distance.max((dx*dx + dy*dy).sqrt());

            // Passive drains while flying
            if !s.landed_on {
                s.oxygen -= OXYGEN_DRAIN;
                s.fuel   -= FUEL_DRAIN_PASSIVE;
                s.fuel   = s.fuel.max(0.0);
                s.oxygen = s.oxygen.max(0.0);
            }

            // ── Planet proximity (landing ring) ────────────────────────────
            // objects_in_radius gives us nearby objects; filter for planets
            let near_planet = c.get_game_object("player").map_or(false, |player_obj| {
                c.objects_in_radius(player_obj, MAX_PLANET_R + SHIP_W * 2.0)
                    .iter()
                    .any(|o| o.tags.contains(&"planet".to_string()))
            });

            if near_planet && !s.landed_on {
                c.run(Action::Show { target: Target::name("land_ring") });
            } else {
                c.run(Action::Hide { target: Target::name("land_ring") });
            }
            if let Some(ring) = c.get_game_object_mut("land_ring") {
                ring.position = (px - 50.0, py - 50.0);
            }

            // ── Landing — collision_between replaces manual distance check ──
            let on_planet = c.collision_between(&Target::name("player"), &Target::tag("planet"));

            if on_planet {
                if !s.landed_on {
                    if speed < SAFE_LAND_SPEED {
                        s.landed_on = true;
                        s.landed_ticks = 0;
                        c.run(Action::SetMomentum { target: Target::name("player"), value: (0.0, 0.0) });
                        c.run(Action::Show { target: Target::name("landed_flash") });
                    } else {
                        // Hard landing: hull damage + bounce
                        let dmg = ((speed - SAFE_LAND_SPEED) / MAX_SPEED * HULL_IMPACT_DAMAGE * 2.0)
                            .min(HULL_IMPACT_DAMAGE);
                        s.hull -= dmg;
                        if let Some(obj) = c.get_game_object("player") {
                            let (mx, my) = (obj.momentum.0 * -0.5, obj.momentum.1 * -0.5);
                            c.run(Action::SetMomentum { target: Target::name("player"), value: (mx, my) });
                        }
                    }
                }
            } else {
                s.landed_on = false;
            }

            // Refill resources while landed
            if s.landed_on {
                s.landed_ticks += 1;
                s.fuel   = (s.fuel   + 0.25).min(100.0);
                s.oxygen = (s.oxygen + 0.20).min(100.0);
                s.hull   = (s.hull   + 0.06).min(100.0);
                if s.landed_ticks == 60 {
                    c.run(Action::Hide { target: Target::name("landed_flash") });
                }
            }

            // ── Low fuel warning — blink via Action::Toggle ────────────────
            s.warn_tick = s.warn_tick.wrapping_add(1);
            if s.fuel < 20.0 && s.warn_tick % 30 == 0 {
                c.run(Action::Toggle { target: Target::name("low_fuel_warn") });
            } else if s.fuel >= 20.0 {
                c.run(Action::Hide { target: Target::name("low_fuel_warn") });
            }

            // ── Update HUD bars ────────────────────────────────────────────
            let fuel_r = if s.fuel   < 20.0 { 255 } else { 80  };
            let fuel_g = if s.fuel   < 20.0 { 80  } else { 200 };
            let oxy_r  = if s.oxygen < 20.0 { 255 } else { 80  };
            let oxy_g  = if s.oxygen < 20.0 { 80  } else { 255 };
            let hull_g = if s.hull   < 30.0 { 40  } else { 120 };

            if let Some(obj) = c.get_game_object_mut("fuel_bar") {
                obj.set_image(bar_to_image(400, 36, s.fuel/100.0,   fuel_r, fuel_g, 255));
            }
            if let Some(obj) = c.get_game_object_mut("oxy_bar") {
                obj.set_image(bar_to_image(400, 36, s.oxygen/100.0, oxy_r,  oxy_g,  160));
            }
            if let Some(obj) = c.get_game_object_mut("hull_bar") {
                obj.set_image(bar_to_image(400, 36, s.hull/100.0,   255,    hull_g, 80));
            }

            // ── Death check ────────────────────────────────────────────────
            s.hull   = s.hull.max(0.0);
            s.oxygen = s.oxygen.max(0.0);
            if s.oxygen <= 0.0 || s.hull <= 0.0 || s.fuel <= 0.0 {
                s.dead = true;
                c.load_scene("gameover");
            }
        });
    })
}

// ─────────────────────────────────────────────────────────────────────────────
pub struct App;

impl App {
    fn new(ctx: &mut Context, _assets: Assets) -> impl Drawable {
        let mut canvas = Canvas::new(ctx, CanvasMode::Landscape);
        canvas.add_scene(build_menu_scene(ctx));
        canvas.add_scene(build_game_scene(ctx));
        canvas.add_scene(build_gameover_scene(ctx));
        canvas.load_scene("menu");
        canvas
    }
}

ramp::run! { |ctx: &mut Context, assets: Assets| { App::new(ctx, assets) } }
