use super::*;

#[cfg(feature = "x86-v4")]
#[cfg(feature = "f16")]
unsafe impl MixedSimd<f16, f16, f16, f32> for V4 {
    const SIMD_WIDTH: usize = 16;

    type LhsN = [f16; 16];
    type RhsN = [f16; 16];
    type DstN = [f16; 16];
    type AccN = [f32; 16];

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
        cast(
            self.avx512f
                ._mm512_fmadd_ps(cast(lhs), cast(rhs), cast(acc)),
        )
    }

    #[inline(always)]
    fn simd_from_lhs(self, lhs: Self::LhsN) -> Self::AccN {
        unsafe { cast(_mm512_cvtph_ps(cast(lhs))) }
    }

    #[inline(always)]
    fn simd_from_rhs(self, rhs: Self::RhsN) -> Self::AccN {
        unsafe { cast(_mm512_cvtph_ps(cast(rhs))) }
    }

    #[inline(always)]
    fn simd_splat(self, lhs: f32) -> Self::AccN {
        cast(self.avx512f._mm512_set1_ps(lhs))
    }

    #[inline(always)]
    fn simd_from_dst(self, dst: Self::DstN) -> Self::AccN {
        unsafe { cast(_mm512_cvtph_ps(cast(dst))) }
    }

    #[inline(always)]
    fn simd_into_dst(self, acc: Self::AccN) -> Self::DstN {
        unsafe { cast(_mm512_cvtps_ph::<_MM_FROUND_CUR_DIRECTION>(cast(acc))) }
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
        cast(self.avx512f._mm512_mul_ps(cast(lhs), cast(rhs)))
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        cast(self.avx512f._mm512_add_ps(cast(lhs), cast(rhs)))
    }
}

#[cfg(feature = "x86-v4")]
unsafe impl MixedSimd<f32, f32, f32, f32> for V4 {
    const SIMD_WIDTH: usize = 16;

    type LhsN = [f32; 16];
    type RhsN = [f32; 16];
    type DstN = [f32; 16];
    type AccN = [f32; 16];

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
        cast(
            self.avx512f
                ._mm512_fmadd_ps(cast(lhs), cast(rhs), cast(acc)),
        )
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
        cast(self.avx512f._mm512_set1_ps(lhs))
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
        cast(self.avx512f._mm512_mul_ps(cast(lhs), cast(rhs)))
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        cast(self.avx512f._mm512_add_ps(cast(lhs), cast(rhs)))
    }
}

#[cfg(feature = "x86-v4")]
unsafe impl MixedSimd<f64, f64, f64, f64> for V4 {
    const SIMD_WIDTH: usize = 8;

    type LhsN = [f64; 8];
    type RhsN = [f64; 8];
    type DstN = [f64; 8];
    type AccN = [f64; 8];

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
        cast(
            self.avx512f
                ._mm512_fmadd_pd(cast(lhs), cast(rhs), cast(acc)),
        )
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
        cast(self.avx512f._mm512_set1_pd(lhs))
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
        cast(self.avx512f._mm512_mul_pd(cast(lhs), cast(rhs)))
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        cast(self.avx512f._mm512_add_pd(cast(lhs), cast(rhs)))
    }
}
