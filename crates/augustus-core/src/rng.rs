// SPDX-License-Identifier: GPL-3.0-only
// Copyright (C) 2026 Project Augustus contributors

use std::convert::Infallible;

use rand_chacha::rand_core::TryRng;
use serde::{Deserialize, Serialize};

const STATE_WORDS: usize = 624;
const TWIST_OFFSET: usize = 397;
const MATRIX_A: u32 = 0x9908_b0df;

/// The deterministic random stream used by EU4-compatible simulation worlds.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Eu4Rng {
    seed: u32,
    count: u32,
    index: u16,
    state: Vec<u32>,
}

impl Eu4Rng {
    pub(crate) fn from_seed(seed: u32) -> Self {
        let mut state = Vec::with_capacity(STATE_WORDS);
        state.push(seed);
        for index in 1..STATE_WORDS {
            let previous = state[index - 1] as i32;
            let mixed = previous ^ (previous >> 30);
            state.push(
                mixed
                    .wrapping_mul(0x6c07_8965_u32 as i32)
                    .wrapping_add(index as i32) as u32,
            );
        }
        Self {
            seed,
            count: 0,
            index: 0,
            state,
        }
    }

    /// Reconstructs the generator represented by EU4's saved seed and draw count.
    pub(crate) fn from_saved_count(seed: u32, count: u32) -> Self {
        let mut rng = Self::from_seed(seed);
        for _ in 0..count {
            rng.next_raw();
        }
        rng.count = count;
        rng
    }

    pub(crate) fn seed(&self) -> u32 {
        self.seed
    }

    pub(crate) fn count(&self) -> u32 {
        self.count
    }

    pub(crate) fn index(&self) -> u16 {
        self.index
    }

    pub(crate) fn next_u32(&mut self) -> u32 {
        let value = self.next_raw();
        self.count = self.count.wrapping_add(1);
        value
    }

    /// Fills one native 1,000-value random block.
    ///
    /// When random generation is forbidden, EU4 fills the destination with zeroes
    /// without advancing either the generator or its draw count.
    pub(crate) fn fill_batch_1000(&mut self, output: &mut [u32; 1000], forbidden: bool) {
        if forbidden {
            output.fill(0);
            return;
        }
        self.count = self.count.wrapping_add(1000);
        for value in output {
            *value = self.next_raw();
        }
    }

    pub(crate) fn is_valid(&self) -> bool {
        self.state.len() == STATE_WORDS && usize::from(self.index) < STATE_WORDS
    }

    fn next_raw(&mut self) -> u32 {
        if self.index == 0 {
            self.twist();
        }
        let index = usize::from(self.index);
        let mut value = self.state[index];
        value ^= ((value as i32) >> 11) as u32;
        value ^= (value & 0xff3a_58ad).wrapping_shl(7);
        value ^= (value & 0xffff_df8c).wrapping_shl(15);
        value ^= ((value as i32) >> 18) as u32;
        self.index = ((index + 1) % STATE_WORDS) as u16;
        value
    }

    fn twist(&mut self) {
        for index in 0..STATE_WORDS {
            let combined = (((self.state[index] ^ self.state[(index + 1) % STATE_WORDS])
                & 0x7fff_ffff)
                ^ self.state[index]) as i32;
            let mut value =
                ((combined >> 1) as u32) ^ self.state[(index + TWIST_OFFSET) % STATE_WORDS];
            if combined % 2 == 1 {
                value ^= MATRIX_A;
            }
            self.state[index] = value;
        }
    }
}

/// Public EU4-compatible random stream with seed/count restoration.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Eu4Random(Eu4Rng);

impl Eu4Random {
    /// Starts a stream from a 32-bit seed.
    pub fn seeded(seed: u32) -> Self {
        Self(Eu4Rng::from_seed(seed))
    }

    /// Reconstructs generator state by replaying the saved number of draws.
    pub fn from_seed_and_count(seed: u32, draw_count: u32) -> Self {
        Self(Eu4Rng::from_saved_count(seed, draw_count))
    }

    /// Produces one value and advances the saved draw count with 32-bit wraparound.
    pub fn next_u32(&mut self) -> u32 {
        self.0.next_u32()
    }

    pub fn seed(&self) -> u32 {
        self.0.seed()
    }

    pub fn draw_count(&self) -> u32 {
        self.0.count()
    }
}

impl TryRng for Eu4Rng {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        Ok(self.next_u32())
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        let low = u64::from(self.next_u32());
        let high = u64::from(self.next_u32());
        Ok(low | (high << 32))
    }

    fn try_fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), Self::Error> {
        let mut words = destination.chunks_exact_mut(4);
        for bytes in &mut words {
            bytes.copy_from_slice(&self.next_u32().to_le_bytes());
        }
        let remainder = words.into_remainder();
        if !remainder.is_empty() {
            let word = self.next_u32().to_le_bytes();
            remainder.copy_from_slice(&word[..remainder.len()]);
        }
        Ok(())
    }
}

pub(crate) enum RandomSource<'a> {
    ChaCha(&'a mut rand_chacha::ChaCha8Rng),
    Eu4(&'a mut Eu4Rng),
}

impl TryRng for RandomSource<'_> {
    type Error = Infallible;

    fn try_next_u32(&mut self) -> Result<u32, Self::Error> {
        match self {
            Self::ChaCha(rng) => match rng.try_next_u32() {
                Ok(value) => Ok(value),
                Err(never) => match never {},
            },
            Self::Eu4(rng) => rng.try_next_u32(),
        }
    }

    fn try_next_u64(&mut self) -> Result<u64, Self::Error> {
        match self {
            Self::ChaCha(rng) => match rng.try_next_u64() {
                Ok(value) => Ok(value),
                Err(never) => match never {},
            },
            Self::Eu4(rng) => rng.try_next_u64(),
        }
    }

    fn try_fill_bytes(&mut self, destination: &mut [u8]) -> Result<(), Self::Error> {
        match self {
            Self::ChaCha(rng) => match rng.try_fill_bytes(destination) {
                Ok(()) => Ok(()),
                Err(never) => match never {},
            },
            Self::Eu4(rng) => rng.try_fill_bytes(destination),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Eu4Random, Eu4Rng};

    #[test]
    fn seeded_streams_match_reference_vectors() {
        let cases = [
            (
                0,
                [
                    503_881_347,
                    1_420_691_967,
                    1_095_544_862,
                    1_428_414_878,
                    1_416_032_285,
                    195_916_887,
                    1_792_333_097,
                    220_690_904,
                    952_081_205,
                    1_459_548_547,
                ],
            ),
            (
                1,
                [
                    462_010_977,
                    662_441_838,
                    297_443_654,
                    462_428_040,
                    145_977_728,
                    1_668_852_308,
                    642_159_105,
                    1_849_836_540,
                    1_599_411_299,
                    667_653_041,
                ],
            ),
            (
                8_351_375,
                [
                    2_131_175_983,
                    1_432_086_561,
                    1_519_919_863,
                    1_488_729_639,
                    674_643_279,
                    856_080_159,
                    1_743_027_350,
                    1_693_715_639,
                    1_062_965_195,
                    339_709_696,
                ],
            ),
        ];

        for (seed, expected) in cases {
            let mut rng = Eu4Rng::from_seed(seed);
            let actual = std::array::from_fn(|_| rng.next_u32());
            assert_eq!(actual, expected);
            assert_eq!(rng.count(), 10);
            assert_eq!(rng.index(), 10);
        }
    }

    #[test]
    fn saved_count_rebuilds_the_generator_without_recounting() {
        for (seed, count) in [(0, 0), (1, 1), (8_351_375, 623), (u32::MAX, 1_001)] {
            let mut uninterrupted = Eu4Rng::from_seed(seed);
            for _ in 0..count {
                uninterrupted.next_u32();
            }
            let mut restored = Eu4Rng::from_saved_count(seed, count);
            assert_eq!(restored, uninterrupted);
            assert_eq!(restored.count(), count);
            assert_eq!(restored.next_u32(), uninterrupted.next_u32());
            assert_eq!(restored.count(), count.wrapping_add(1));
        }
    }

    #[test]
    fn batch_generation_updates_count_and_forbidden_generation_is_inert() {
        let mut rng = Eu4Rng::from_seed(8_351_375);
        let mut output = [0; 1000];
        rng.fill_batch_1000(&mut output, false);
        assert_eq!(
            &output[..10],
            &[
                2_131_175_983,
                1_432_086_561,
                1_519_919_863,
                1_488_729_639,
                674_643_279,
                856_080_159,
                1_743_027_350,
                1_693_715_639,
                1_062_965_195,
                339_709_696,
            ]
        );
        assert_eq!(rng.count(), 1000);
        assert_eq!(rng.index(), 376);

        let mut unchanged = rng.clone();
        let mut forbidden = [u32::MAX; 1000];
        unchanged.fill_batch_1000(&mut forbidden, true);
        assert!(forbidden.iter().all(|value| *value == 0));
        assert_eq!(unchanged, rng);
    }

    fn state_fingerprint(rng: &Eu4Rng) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in rng
            .state
            .iter()
            .flat_map(|word| word.to_le_bytes())
            .chain((u32::from(rng.index)).to_le_bytes())
        {
            hash = (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        hash
    }

    #[test]
    fn restored_state_matches_reference_fingerprints_at_refill_boundaries() {
        let cases = [
            (0, 0, 0, "8252b69738bb3391", 503_881_347),
            (0, 1, 1, "d5bd9eb2fe07c16e", 1_420_691_967),
            (0, 623, 623, "255baf5ea50e2536", 1_336_570_068),
            (0, 624, 0, "75b84abb543d7dff", 1_786_401_012),
            (0, 625, 1, "284b8b9b538b905f", 1_973_643_177),
            (0, 1_000, 376, "92cf37b55fc500a1", 2_107_979_823),
            (0, 1_001, 377, "2176edbdac2abfdc", 827_613_586),
            (1, 0, 0, "e1970ee5b826b84f", 462_010_977),
            (1, 1, 1, "63c3100dafcdf2b3", 662_441_838),
            (1, 623, 623, "903b5581583e0967", 486_038_229),
            (1, 624, 0, "c3c8640559983622", 1_644_266_106),
            (1, 625, 1, "48606962389f2c1c", 1_677_485_631),
            (1, 1_000, 376, "e0dbf897ae2baa4a", 585_172_832),
            (1, 1_001, 377, "5234428f61c5eb0f", 963_811_212),
            (u32::MAX, 0, 0, "e34fc6c09c7462ad", 747_063_145),
            (u32::MAX, 1, 1, "cfe21265f6d2c5d1", 1_420_691_967),
            (u32::MAX, 623, 623, "a194efcff0db80f5", 1_370_624_585),
            (u32::MAX, 624, 0, "2fe7665da09d0940", 1_786_401_012),
            (u32::MAX, 625, 1, "2d5fe19e714b9faf", 1_973_643_177),
            (u32::MAX, 1_000, 376, "95e410d8d1abc651", 2_107_979_823),
            (u32::MAX, 1_001, 377, "248bc6e11e11858c", 827_613_586),
            (8_351_375, 0, 0, "1832a6487d7fc262", 2_131_175_983),
            (8_351_375, 1, 1, "63c95fd1914f90f8", 1_432_086_561),
            (8_351_375, 623, 623, "14808ea086ed6420", 226_615_163),
            (8_351_375, 624, 0, "03c40bd9e7854d89", 1_805_961_901),
            (8_351_375, 625, 1, "e5fe7f8d2ec544dd", 1_452_152_108),
            (8_351_375, 1_000, 376, "4c8331e7e34c21df", 1_874_811_898),
            (8_351_375, 1_001, 377, "db2ae7f02fb1e11a", 312_438_070),
        ];

        for (seed, count, expected_index, expected_state, expected_next) in cases {
            let mut rng = Eu4Rng::from_saved_count(seed, count);
            assert_eq!(rng.seed(), seed);
            assert_eq!(rng.count(), count);
            assert_eq!(rng.index(), expected_index);
            assert_eq!(
                state_fingerprint(&rng),
                u64::from_str_radix(expected_state, 16).unwrap()
            );
            assert_eq!(rng.next_u32(), expected_next);
            assert_eq!(rng.count(), count.wrapping_add(1));
        }
    }

    #[test]
    fn draw_counter_wraps_without_changing_the_word_stream() {
        let mut rng = Eu4Rng::from_seed(0);
        rng.count = u32::MAX;
        assert_eq!(rng.next_u32(), 503_881_347);
        assert_eq!(rng.count(), 0);
    }

    #[test]
    fn public_random_wrapper_restores_the_eu4_stream() {
        let mut random = Eu4Random::from_seed_and_count(8_351_375, 624);
        assert_eq!(random.seed(), 8_351_375);
        assert_eq!(random.draw_count(), 624);
        assert_eq!(random.next_u32(), 1_805_961_901);
        assert_eq!(random.draw_count(), 625);
    }
}
