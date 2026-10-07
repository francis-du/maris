use super::*;

unsafe impl MixedSimd<c32, c32, c32, c32> for V3 {
    const SIMD_WIDTH: usize = 4;

    type LhsN = [c32; 4];
    type RhsN = [c32; 4];
    type DstN = [c32; 4];
    type AccN = [c32; 4];

    #[inline]
    fn try_new() -> Option<Self> {
        Self::try_new()
    }

    #[inline(always)]
    fn mult(self, lhs: c32, rhs: c32) -> c32 {
        lhs * rhs
    }

    #[inline(always)]
    fn mult_add(self, lhs: c32, rhs: c32, acc: c32) -> c32 {
        lhs * rhs + acc
    }

    #[inline(always)]
    fn from_lhs(self, lhs: c32) -> c32 {
        lhs
    }

    #[inline(always)]
    fn from_rhs(self, rhs: c32) -> c32 {
        rhs
    }

    #[inline(always)]
    fn from_dst(self, dst: c32) -> c32 {
        dst
    }

    #[inline(always)]
    fn into_dst(self, acc: c32) -> c32 {
        acc
    }

    #[inline(always)]
    fn simd_mult_add(self, lhs: Self::AccN, rhs: Self::AccN, acc: Self::AccN) -> Self::AccN {
        unsafe {
            let ab = cast(lhs);
            let xy = cast(rhs);

            let yx = _mm256_permute_ps::<0b10_11_00_01>(xy);
            let aa = _mm256_moveldup_ps(ab);
            let bb = _mm256_movehdup_ps(ab);

            cast(_mm256_fmaddsub_ps(
                aa,
                xy,
                _mm256_fmaddsub_ps(bb, yx, cast(acc)),
            ))
        }
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
    fn simd_splat(self, lhs: c32) -> Self::AccN {
        cast(self.avx._mm256_set1_pd(cast(lhs)))
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
    fn add(self, lhs: c32, rhs: c32) -> c32 {
        lhs + rhs
    }

    #[inline(always)]
    fn simd_mul(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        unsafe {
            let ab = cast(lhs);
            let xy = cast(rhs);

            let yx = _mm256_permute_ps::<0b10_11_00_01>(xy);
            let aa = _mm256_moveldup_ps(ab);
            let bb = _mm256_movehdup_ps(ab);

            cast(_mm256_fmaddsub_ps(aa, xy, _mm256_mul_ps(bb, yx)))
        }
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        cast(self.avx._mm256_add_ps(cast(lhs), cast(rhs)))
    }
}

unsafe impl MixedSimd<c64, c64, c64, c64> for V3 {
    const SIMD_WIDTH: usize = 2;

    type LhsN = [c64; 2];
    type RhsN = [c64; 2];
    type DstN = [c64; 2];
    type AccN = [c64; 2];

    #[inline]
    fn try_new() -> Option<Self> {
        Self::try_new()
    }

    #[inline(always)]
    fn mult(self, lhs: c64, rhs: c64) -> c64 {
        lhs * rhs
    }

    #[inline(always)]
    fn mult_add(self, lhs: c64, rhs: c64, acc: c64) -> c64 {
        lhs * rhs + acc
    }

    #[inline(always)]
    fn from_lhs(self, lhs: c64) -> c64 {
        lhs
    }

    #[inline(always)]
    fn from_rhs(self, rhs: c64) -> c64 {
        rhs
    }

    #[inline(always)]
    fn from_dst(self, dst: c64) -> c64 {
        dst
    }

    #[inline(always)]
    fn into_dst(self, acc: c64) -> c64 {
        acc
    }

    #[inline(always)]
    fn simd_mult_add(self, lhs: Self::AccN, rhs: Self::AccN, acc: Self::AccN) -> Self::AccN {
        unsafe {
            let ab = cast(lhs);
            let xy = cast(rhs);

            let yx = _mm256_permute_pd::<0b0101>(xy);
            let aa = _mm256_unpacklo_pd(ab, ab);
            let bb = _mm256_unpackhi_pd(ab, ab);

            cast(_mm256_fmaddsub_pd(
                aa,
                xy,
                _mm256_fmaddsub_pd(bb, yx, cast(acc)),
            ))
        }
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
    fn simd_splat(self, lhs: c64) -> Self::AccN {
        cast([lhs; 2])
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
    fn add(self, lhs: c64, rhs: c64) -> c64 {
        lhs + rhs
    }

    #[inline(always)]
    fn simd_mul(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        unsafe {
            let ab = cast(lhs);
            let xy = cast(rhs);

            let yx = _mm256_permute_pd::<0b0101>(xy);
            let aa = _mm256_unpacklo_pd(ab, ab);
            let bb = _mm256_unpackhi_pd(ab, ab);

            cast(_mm256_fmaddsub_pd(aa, xy, _mm256_mul_pd(bb, yx)))
        }
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        cast(self.avx._mm256_add_pd(cast(lhs), cast(rhs)))
    }
}

#[cfg(feature = "x86-v4")]
unsafe impl MixedSimd<c32, c32, c32, c32> for V4 {
    const SIMD_WIDTH: usize = 8;

    type LhsN = [c32; 8];
    type RhsN = [c32; 8];
    type DstN = [c32; 8];
    type AccN = [c32; 8];

    #[inline]
    fn try_new() -> Option<Self> {
        Self::try_new()
    }

    #[inline(always)]
    fn mult(self, lhs: c32, rhs: c32) -> c32 {
        lhs * rhs
    }

    #[inline(always)]
    fn mult_add(self, lhs: c32, rhs: c32, acc: c32) -> c32 {
        lhs * rhs + acc
    }

    #[inline(always)]
    fn from_lhs(self, lhs: c32) -> c32 {
        lhs
    }

    #[inline(always)]
    fn from_rhs(self, rhs: c32) -> c32 {
        rhs
    }

    #[inline(always)]
    fn from_dst(self, dst: c32) -> c32 {
        dst
    }

    #[inline(always)]
    fn into_dst(self, acc: c32) -> c32 {
        acc
    }

    #[inline(always)]
    fn simd_mult_add(self, lhs: Self::AccN, rhs: Self::AccN, acc: Self::AccN) -> Self::AccN {
        unsafe {
            let ab = cast(lhs);
            let xy = cast(rhs);

            let yx = _mm512_permute_ps::<0b10_11_00_01>(xy);
            let aa = _mm512_moveldup_ps(ab);
            let bb = _mm512_movehdup_ps(ab);

            cast(_mm512_fmaddsub_ps(
                aa,
                xy,
                _mm512_fmaddsub_ps(bb, yx, cast(acc)),
            ))
        }
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
    fn simd_splat(self, lhs: c32) -> Self::AccN {
        cast(self.avx512f._mm512_set1_pd(cast(lhs)))
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
    fn add(self, lhs: c32, rhs: c32) -> c32 {
        lhs + rhs
    }

    #[inline(always)]
    fn simd_mul(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        unsafe {
            let ab = cast(lhs);
            let xy = cast(rhs);

            let yx = _mm512_permute_ps::<0b10_11_00_01>(xy);
            let aa = _mm512_moveldup_ps(ab);
            let bb = _mm512_movehdup_ps(ab);

            cast(_mm512_fmaddsub_ps(aa, xy, _mm512_mul_ps(bb, yx)))
        }
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        cast(self.avx512f._mm512_add_ps(cast(lhs), cast(rhs)))
    }
}

#[cfg(feature = "x86-v4")]
unsafe impl MixedSimd<c64, c64, c64, c64> for V4 {
    const SIMD_WIDTH: usize = 4;

    type LhsN = [c64; 4];
    type RhsN = [c64; 4];
    type DstN = [c64; 4];
    type AccN = [c64; 4];

    #[inline]
    fn try_new() -> Option<Self> {
        Self::try_new()
    }

    #[inline(always)]
    fn mult(self, lhs: c64, rhs: c64) -> c64 {
        lhs * rhs
    }

    #[inline(always)]
    fn mult_add(self, lhs: c64, rhs: c64, acc: c64) -> c64 {
        lhs * rhs + acc
    }

    #[inline(always)]
    fn from_lhs(self, lhs: c64) -> c64 {
        lhs
    }

    #[inline(always)]
    fn from_rhs(self, rhs: c64) -> c64 {
        rhs
    }

    #[inline(always)]
    fn from_dst(self, dst: c64) -> c64 {
        dst
    }

    #[inline(always)]
    fn into_dst(self, acc: c64) -> c64 {
        acc
    }

    #[inline(always)]
    fn simd_mult_add(self, lhs: Self::AccN, rhs: Self::AccN, acc: Self::AccN) -> Self::AccN {
        unsafe {
            let ab = cast(lhs);
            let xy = cast(rhs);

            let yx = _mm512_permute_pd::<0b01010101>(xy);
            let aa = _mm512_unpacklo_pd(ab, ab);
            let bb = _mm512_unpackhi_pd(ab, ab);

            cast(_mm512_fmaddsub_pd(
                aa,
                xy,
                _mm512_fmaddsub_pd(bb, yx, cast(acc)),
            ))
        }
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
    fn simd_splat(self, lhs: c64) -> Self::AccN {
        cast([lhs; 4])
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
    fn add(self, lhs: c64, rhs: c64) -> c64 {
        lhs + rhs
    }

    #[inline(always)]
    fn simd_mul(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        unsafe {
            let ab = cast(lhs);
            let xy = cast(rhs);

            let yx = _mm512_permute_pd::<0b01010101>(xy);
            let aa = _mm512_unpacklo_pd(ab, ab);
            let bb = _mm512_unpackhi_pd(ab, ab);

            cast(_mm512_fmaddsub_pd(aa, xy, _mm512_mul_pd(bb, yx)))
        }
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        cast(self.avx512f._mm512_add_pd(cast(lhs), cast(rhs)))
    }
}
