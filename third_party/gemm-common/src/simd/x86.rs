use super::*;
#[cfg(target_arch = "x86")]
use core::arch::x86::*;
#[cfg(target_arch = "x86_64")]
use core::arch::x86_64::*;

#[inline(always)]
pub unsafe fn v3_fmaf(a: f32, b: f32, c: f32) -> f32 {
    #[cfg(feature = "std")]
    {
        f32::mul_add(a, b, c)
    }
    #[cfg(not(feature = "std"))]
    {
        libm::fmaf(a, b, c)
    }
}

#[inline(always)]
pub unsafe fn v3_fma(a: f64, b: f64, c: f64) -> f64 {
    #[cfg(feature = "std")]
    {
        f64::mul_add(a, b, c)
    }
    #[cfg(not(feature = "std"))]
    {
        libm::fma(a, b, c)
    }
}

#[derive(Copy, Clone)]
pub struct Sse;
#[derive(Copy, Clone)]
pub struct Avx;
#[derive(Copy, Clone)]
pub struct Fma;

#[cfg(feature = "x86-v4")]
#[derive(Copy, Clone)]
pub struct Avx512f;

impl Simd for Sse {
    #[inline]
    #[target_feature(enable = "sse,sse2")]
    unsafe fn vectorize<F: NullaryFnOnce>(f: F) -> F::Output {
        f.call()
    }
}

impl Simd for Avx {
    #[inline]
    #[target_feature(enable = "avx")]
    unsafe fn vectorize<F: NullaryFnOnce>(f: F) -> F::Output {
        f.call()
    }
}

impl Simd for Fma {
    #[inline]
    #[target_feature(enable = "fma")]
    unsafe fn vectorize<F: NullaryFnOnce>(f: F) -> F::Output {
        f.call()
    }
}

#[cfg(feature = "x86-v4")]
impl Simd for Avx512f {
    #[inline]
    #[target_feature(enable = "avx512f")]
    unsafe fn vectorize<F: NullaryFnOnce>(f: F) -> F::Output {
        f.call()
    }
}

#[derive(Debug, Copy, Clone)]
pub struct V3Half {
    __private: (),
}

#[cfg(feature = "f16")]
pulp::simd_type! {
    pub struct V3 {
        pub sse: "sse",
        pub sse2: "sse2",
        pub fxsr: "fxsr",
        pub sse3: "sse3",
        pub ssse3: "ssse3",
        pub sse4_1: "sse4.1",
        pub sse4_2: "sse4.2",
        pub avx: "avx",
        pub avx2: "avx2",
        pub fma: "fma",
        pub f16c: "f16c",
    }
}

#[cfg(feature = "x86-v4")]
#[cfg(feature = "f16")]
pulp::simd_type! {
    pub struct V4 {
        pub sse: "sse",
        pub sse2: "sse2",
        pub fxsr: "fxsr",
        pub sse3: "sse3",
        pub ssse3: "ssse3",
        pub sse4_1: "sse4.1",
        pub sse4_2: "sse4.2",
        pub avx: "avx",
        pub avx2: "avx2",
        pub fma: "fma",
        pub f16c: "f16c",
        pub avx512f: "avx512f",
    }
}

#[cfg(not(feature = "f16"))]
pulp::simd_type! {
    pub struct V3 {
        pub sse: "sse",
        pub sse2: "sse2",
        pub fxsr: "fxsr",
        pub sse3: "sse3",
        pub ssse3: "ssse3",
        pub sse4_1: "sse4.1",
        pub sse4_2: "sse4.2",
        pub avx: "avx",
        pub avx2: "avx2",
        pub fma: "fma",
    }
}

#[cfg(feature = "x86-v4")]
#[cfg(not(feature = "f16"))]
pulp::simd_type! {
    pub struct V4 {
        pub sse: "sse",
        pub sse2: "sse2",
        pub fxsr: "fxsr",
        pub sse3: "sse3",
        pub ssse3: "ssse3",
        pub sse4_1: "sse4.1",
        pub sse4_2: "sse4.2",
        pub avx: "avx",
        pub avx2: "avx2",
        pub fma: "fma",
        pub avx512f: "avx512f",
    }
}

impl Simd for V3Half {
    unsafe fn vectorize<F: NullaryFnOnce>(f: F) -> F::Output {
        f.call()
    }
}

#[cfg(feature = "f16")]
unsafe impl MixedSimd<f16, f16, f16, f32> for V3Half {
    const SIMD_WIDTH: usize = 4;

    type LhsN = [f16; 4];
    type RhsN = [f16; 4];
    type DstN = [f16; 4];
    type AccN = [f32; 4];

    #[inline]
    fn try_new() -> Option<Self> {
        Some(Self { __private: () })
    }

    #[inline(always)]
    fn mult(self, lhs: f32, rhs: f32) -> f32 {
        lhs * rhs
    }

    #[inline(always)]
    fn mult_add(self, lhs: f32, rhs: f32, acc: f32) -> f32 {
        unsafe { v3_fmaf(lhs, rhs, acc) }
    }

    #[inline(always)]
    fn from_lhs(self, _lhs: f16) -> f32 {
        todo!()
    }

    #[inline(always)]
    fn from_rhs(self, _rhs: f16) -> f32 {
        todo!()
    }

    #[inline(always)]
    fn from_dst(self, _dst: f16) -> f32 {
        todo!()
    }

    #[inline(always)]
    fn into_dst(self, _acc: f32) -> f16 {
        todo!()
    }

    #[inline(always)]
    fn simd_mult_add(self, _lhs: Self::AccN, _rhs: Self::AccN, _acc: Self::AccN) -> Self::AccN {
        todo!()
    }

    #[inline(always)]
    fn simd_from_lhs(self, _lhs: Self::LhsN) -> Self::AccN {
        todo!()
    }

    #[inline(always)]
    fn simd_from_rhs(self, _rhs: Self::RhsN) -> Self::AccN {
        todo!()
    }

    #[inline(always)]
    fn simd_splat(self, _lhs: f32) -> Self::AccN {
        todo!()
    }

    #[inline(always)]
    fn simd_from_dst(self, dst: Self::DstN) -> Self::AccN {
        unsafe { cast(_mm_cvtph_ps(cast([dst, [f16::ZERO; 4]]))) }
    }

    #[inline(always)]
    fn simd_into_dst(self, _acc: Self::AccN) -> Self::DstN {
        todo!()
    }

    #[inline(always)]
    fn vectorize<F: NullaryFnOnce>(self, _f: F) -> F::Output {
        todo!()
    }

    #[inline(always)]
    fn add(self, _lhs: f32, _rhs: f32) -> f32 {
        todo!()
    }

    #[inline(always)]
    fn simd_mul(self, _lhs: Self::AccN, _rhs: Self::AccN) -> Self::AccN {
        todo!()
    }

    #[inline(always)]
    fn simd_add(self, _lhs: Self::AccN, _rhs: Self::AccN) -> Self::AccN {
        todo!()
    }
}

#[cfg(feature = "f16")]
unsafe impl MixedSimd<f16, f16, f16, f32> for V3 {
    const SIMD_WIDTH: usize = 8;

    type LhsN = [f16; 8];
    type RhsN = [f16; 8];
    type DstN = [f16; 8];
    type AccN = [f32; 8];

    #[inline]
    fn try_new() -> Option<Self> {
        Self::try_new()
    }

    #[inline(always)]
    fn mult(self, lhs: f32, rhs: f32) -> f32 {
        lhs * rhs
    }

    #[inline(always)]
    fn mult_add(self, lhs: f32, rhs: f32, acc: f32) -> f32 {
        unsafe { v3_fmaf(lhs, rhs, acc) }
    }

    #[inline(always)]
    fn from_lhs(self, lhs: f16) -> f32 {
        unsafe { pulp::cast_lossy(_mm_cvtph_ps(self.sse2._mm_set1_epi16(cast(lhs)))) }
    }

    #[inline(always)]
    fn from_rhs(self, rhs: f16) -> f32 {
        unsafe { pulp::cast_lossy(_mm_cvtph_ps(self.sse2._mm_set1_epi16(cast(rhs)))) }
    }

    #[inline(always)]
    fn from_dst(self, dst: f16) -> f32 {
        unsafe { pulp::cast_lossy(_mm_cvtph_ps(self.sse2._mm_set1_epi16(cast(dst)))) }
    }

    #[inline(always)]
    fn into_dst(self, acc: f32) -> f16 {
        unsafe {
            pulp::cast_lossy(_mm_cvtps_ph::<_MM_FROUND_CUR_DIRECTION>(
                self.sse._mm_load_ss(&acc),
            ))
        }
    }

    #[inline(always)]
    fn simd_mult_add(self, lhs: Self::AccN, rhs: Self::AccN, acc: Self::AccN) -> Self::AccN {
        cast(self.fma._mm256_fmadd_ps(cast(lhs), cast(rhs), cast(acc)))
    }

    #[inline(always)]
    fn simd_from_lhs(self, lhs: Self::LhsN) -> Self::AccN {
        unsafe { cast(_mm256_cvtph_ps(cast(lhs))) }
    }

    #[inline(always)]
    fn simd_from_rhs(self, rhs: Self::RhsN) -> Self::AccN {
        unsafe { cast(_mm256_cvtph_ps(cast(rhs))) }
    }

    #[inline(always)]
    fn simd_splat(self, lhs: f32) -> Self::AccN {
        cast(self.avx._mm256_set1_ps(lhs))
    }

    #[inline(always)]
    fn simd_from_dst(self, dst: Self::DstN) -> Self::AccN {
        unsafe { cast(_mm256_cvtph_ps(cast(dst))) }
    }

    #[inline(always)]
    fn simd_into_dst(self, acc: Self::AccN) -> Self::DstN {
        unsafe { cast(_mm256_cvtps_ph::<_MM_FROUND_CUR_DIRECTION>(cast(acc))) }
    }

    #[inline(always)]
    fn vectorize<F: NullaryFnOnce>(self, f: F) -> F::Output {
        self.vectorize(f)
    }

    #[inline(always)]
    fn add(self, lhs: f32, rhs: f32) -> f32 {
        lhs + rhs
    }

    #[inline(always)]
    fn simd_mul(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        cast(self.avx._mm256_mul_ps(cast(lhs), cast(rhs)))
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        cast(self.avx._mm256_add_ps(cast(lhs), cast(rhs)))
    }
}

unsafe impl MixedSimd<f32, f32, f32, f32> for V3 {
    const SIMD_WIDTH: usize = 8;

    type LhsN = [f32; 8];
    type RhsN = [f32; 8];
    type DstN = [f32; 8];
    type AccN = [f32; 8];

    #[inline]
    fn try_new() -> Option<Self> {
        Self::try_new()
    }

    #[inline(always)]
    fn mult(self, lhs: f32, rhs: f32) -> f32 {
        lhs * rhs
    }

    #[inline(always)]
    fn mult_add(self, lhs: f32, rhs: f32, acc: f32) -> f32 {
        unsafe { v3_fmaf(lhs, rhs, acc) }
    }

    #[inline(always)]
    fn from_lhs(self, lhs: f32) -> f32 {
        lhs
    }

    #[inline(always)]
    fn from_rhs(self, rhs: f32) -> f32 {
        rhs
    }

    #[inline(always)]
    fn from_dst(self, dst: f32) -> f32 {
        dst
    }

    #[inline(always)]
    fn into_dst(self, acc: f32) -> f32 {
        acc
    }

    #[inline(always)]
    fn simd_mult_add(self, lhs: Self::AccN, rhs: Self::AccN, acc: Self::AccN) -> Self::AccN {
        cast(self.fma._mm256_fmadd_ps(cast(lhs), cast(rhs), cast(acc)))
    }

    #[inline(always)]
    fn simd_from_lhs(self, lhs: Self::LhsN) -> Self::AccN {
        lhs
    }

    #[inline(always)]
    fn simd_from_rhs(self, rhs: Self::RhsN) -> Self::AccN {
        rhs
    }

    #[inline(always)]
    fn simd_splat(self, lhs: f32) -> Self::AccN {
        cast(self.avx._mm256_set1_ps(lhs))
    }

    #[inline(always)]
    fn simd_from_dst(self, dst: Self::DstN) -> Self::AccN {
        dst
    }

    #[inline(always)]
    fn simd_into_dst(self, acc: Self::AccN) -> Self::DstN {
        acc
    }

    #[inline(always)]
    fn vectorize<F: NullaryFnOnce>(self, f: F) -> F::Output {
        self.vectorize(f)
    }

    #[inline(always)]
    fn add(self, lhs: f32, rhs: f32) -> f32 {
        lhs + rhs
    }

    #[inline(always)]
    fn simd_mul(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        cast(self.avx._mm256_mul_ps(cast(lhs), cast(rhs)))
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        cast(self.avx._mm256_add_ps(cast(lhs), cast(rhs)))
    }
}

unsafe impl MixedSimd<f64, f64, f64, f64> for V3 {
    const SIMD_WIDTH: usize = 4;

    type LhsN = [f64; 4];
    type RhsN = [f64; 4];
    type DstN = [f64; 4];
    type AccN = [f64; 4];

    #[inline]
    fn try_new() -> Option<Self> {
        Self::try_new()
    }

    #[inline(always)]
    fn mult(self, lhs: f64, rhs: f64) -> f64 {
        lhs * rhs
    }

    #[inline(always)]
    fn mult_add(self, lhs: f64, rhs: f64, acc: f64) -> f64 {
        unsafe { v3_fma(lhs, rhs, acc) }
    }

    #[inline(always)]
    fn from_lhs(self, lhs: f64) -> f64 {
        lhs
    }

    #[inline(always)]
    fn from_rhs(self, rhs: f64) -> f64 {
        rhs
    }

    #[inline(always)]
    fn from_dst(self, dst: f64) -> f64 {
        dst
    }

    #[inline(always)]
    fn into_dst(self, acc: f64) -> f64 {
        acc
    }

    #[inline(always)]
    fn simd_mult_add(self, lhs: Self::AccN, rhs: Self::AccN, acc: Self::AccN) -> Self::AccN {
        cast(self.fma._mm256_fmadd_pd(cast(lhs), cast(rhs), cast(acc)))
    }

    #[inline(always)]
    fn simd_from_lhs(self, lhs: Self::LhsN) -> Self::AccN {
        lhs
    }

    #[inline(always)]
    fn simd_from_rhs(self, rhs: Self::RhsN) -> Self::AccN {
        rhs
    }

    #[inline(always)]
    fn simd_splat(self, lhs: f64) -> Self::AccN {
        cast(self.avx._mm256_set1_pd(lhs))
    }

    #[inline(always)]
    fn simd_from_dst(self, dst: Self::DstN) -> Self::AccN {
        dst
    }

    #[inline(always)]
    fn simd_into_dst(self, acc: Self::AccN) -> Self::DstN {
        acc
    }

    #[inline(always)]
    fn vectorize<F: NullaryFnOnce>(self, f: F) -> F::Output {
        self.vectorize(f)
    }

    #[inline(always)]
    fn add(self, lhs: f64, rhs: f64) -> f64 {
        lhs + rhs
    }

    #[inline(always)]
    fn simd_mul(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        cast(self.avx._mm256_mul_pd(cast(lhs), cast(rhs)))
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        cast(self.avx._mm256_add_pd(cast(lhs), cast(rhs)))
    }
}

impl Simd for V3 {
    #[inline(always)]
    unsafe fn vectorize<F: NullaryFnOnce>(f: F) -> F::Output {
        Self::new_unchecked().vectorize(f)
    }
}

#[cfg(feature = "x86-v4")]
impl Simd for V4 {
    #[inline(always)]
    unsafe fn vectorize<F: NullaryFnOnce>(f: F) -> F::Output {
        Self::new_unchecked().vectorize(f)
    }
}

mod complex;
#[cfg(feature = "x86-v4")]
mod wide;
