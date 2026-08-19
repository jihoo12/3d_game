//! Small, dependency-free noise helpers. Kept in-house instead of pulling in
//! the `noise` crate since terrain gen is the only place that needs it and
//! this is ~30 lines; swap for `noise`'s Perlin/Simplex if terrain needs to
//! get fancier (3D noise, ridged multifractal, etc.) later.

/// Deterministic hash of an integer 2D coordinate into [0, 1). The same
/// coordinate always hashes to the same value, so terrain (and anything else
/// built on this, like tree scattering) is fully reproducible run to run —
/// no seed to store, no RNG state to thread through.
fn hash2(x: i32, z: i32) -> f32 {
    let mut h = (x.wrapping_mul(374_761_393)).wrapping_add(z.wrapping_mul(668_265_263));
    h = (h ^ (h >> 13)).wrapping_mul(1_274_126_177);
    h ^= h >> 16;
    (h & 0x00FF_FFFF) as f32 / 0x0100_0000 as f32
}

/// Smooth (ease-in-ease-out) interpolation factor, cheaper than an actual
/// cosine and avoids the sharp creases a plain linear lerp would leave at
/// every integer grid line.
fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

/// Bilinearly-interpolated value noise at a non-integer 2D coordinate,
/// built from `hash2` at the four surrounding integer corners. Range is
/// roughly [0, 1).
fn value_noise2(x: f32, z: f32) -> f32 {
    let x0 = x.floor() as i32;
    let z0 = z.floor() as i32;
    let tx = smoothstep(x - x0 as f32);
    let tz = smoothstep(z - z0 as f32);

    let h00 = hash2(x0, z0);
    let h10 = hash2(x0 + 1, z0);
    let h01 = hash2(x0, z0 + 1);
    let h11 = hash2(x0 + 1, z0 + 1);

    let a = h00 + (h10 - h00) * tx;
    let b = h01 + (h11 - h01) * tx;
    a + (b - a) * tz
}

/// Fractal Brownian motion: stacks several octaves of `value_noise2` at
/// increasing frequency and decreasing amplitude, so the result has broad
/// rolling shape from the low octaves plus smaller bumpy detail from the
/// high ones — this is what actually fixes the old heightmap's obviously
/// repeating sine-wave look. Returns roughly [0, 1).
pub fn fbm(x: f32, z: f32, octaves: u32, lacunarity: f32, gain: f32) -> f32 {
    let mut amplitude = 0.5;
    let mut frequency = 1.0;
    let mut sum = 0.0;
    let mut max_amplitude = 0.0;

    for _ in 0..octaves {
        sum += value_noise2(x * frequency, z * frequency) * amplitude;
        max_amplitude += amplitude;
        amplitude *= gain;
        frequency *= lacunarity;
    }

    sum / max_amplitude
}

/// Cheap deterministic yes/no check for scattering features (trees, etc.)
/// across the grid. `seed` decorrelates it from whatever noise generated the
/// terrain shape itself, so placement doesn't visibly line up with the
/// height/moisture contours.
pub fn scatter_check(x: i32, z: i32, seed: i32, density: f32) -> bool {
    hash2(x.wrapping_add(seed), z.wrapping_sub(seed)) < density
}
