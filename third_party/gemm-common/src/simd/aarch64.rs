use super::*;
use core::arch::aarch64::*;
use core::arch::asm;
#[allow(unused_imports)]
use core::mem::transmute;
use core::mem::MaybeUninit;
use core::ptr;

#[inline(always)]
pub unsafe fn neon_fmaf(a: f32, b: f32, c: f32) -> f32 {
    #[cfg(feature = "std")]
    {
        f32::mul_add(a, b, c)
    }
    #[cfg(not(feature = "std"))]
    {
        a * b + c
    }
}

#[inline(always)]
pub unsafe fn neon_fma(a: f64, b: f64, c: f64) -> f64 {
    #[cfg(feature = "std")]
    {
        f64::mul_add(a, b, c)
    }
    #[cfg(not(feature = "std"))]
    {
        a * b + c
    }
}

#[target_feature(enable = "fp16,neon")]
#[inline]
pub unsafe fn f16_to_f32_fp16(i: u16) -> f32 {
    let result: f32;
    asm!(
    "fcvt {0:s}, {1:h}",
    out(vreg) result,
    in(vreg) i,
    options(pure, nomem, nostack));
    result
}

#[target_feature(enable = "fp16,neon")]
#[inline]
pub unsafe fn f32_to_f16_fp16(f: f32) -> u16 {
    let result: u16;
    asm!(
    "fcvt {0:h}, {1:s}",
    out(vreg) result,
    in(vreg) f,
    options(pure, nomem, nostack));
    result
}

#[target_feature(enable = "fp16,neon")]
#[inline]
pub unsafe fn f16x4_to_f32x4_fp16(v: &[u16; 4]) -> [f32; 4] {
    let mut vec = MaybeUninit::<uint16x4_t>::uninit();
    ptr::copy_nonoverlapping(v.as_ptr(), vec.as_mut_ptr().cast(), 4);
    let result: float32x4_t;
    asm!(
    "fcvtl {0:v}.4s, {1:v}.4h",
    out(vreg) result,
    in(vreg) vec.assume_init(),
    options(pure, nomem, nostack));
    *(&result as *const float32x4_t).cast()
}

#[target_feature(enable = "fp16,neon")]
#[inline]
pub unsafe fn f32x4_to_f16x4_fp16(v: &[f32; 4]) -> [u16; 4] {
    let mut vec = MaybeUninit::<float32x4_t>::uninit();
    ptr::copy_nonoverlapping(v.as_ptr(), vec.as_mut_ptr().cast(), 4);
    let result: uint16x4_t;
    asm!(
    "fcvtn {0:v}.4h, {1:v}.4s",
    out(vreg) result,
    in(vreg) vec.assume_init(),
    options(pure, nomem, nostack));
    *(&result as *const uint16x4_t).cast()
}

#[target_feature(enable = "fp16")]
#[inline]
pub unsafe fn add_f16_fp16(a: u16, b: u16) -> u16 {
    let result: u16;
    asm!(
    "fadd {0:h}, {1:h}, {2:h}",
    out(vreg) result,
    in(vreg) a,
    in(vreg) b,
    options(pure, nomem, nostack));
    result
}

#[target_feature(enable = "fp16")]
#[inline]
pub unsafe fn fmaq_f16(mut a: u16, b: u16, c: u16) -> u16 {
    asm!(
    "fmadd {0:h}, {1:h}, {2:h}, {0:h}",
    inout(vreg) a,
    in(vreg) b,
    in(vreg) c,
    options(pure, nomem, nostack));
    a
}

#[target_feature(enable = "fp16")]
#[inline]
pub unsafe fn multiply_f16_fp16(a: u16, b: u16) -> u16 {
    let result: u16;
    asm!(
    "fmul {0:h}, {1:h}, {2:h}",
    out(vreg) result,
    in(vreg) a,
    in(vreg) b,
    options(pure, nomem, nostack));
    result
}

#[allow(non_camel_case_types)]
type float16x8_t = uint16x8_t;

/// Floating point multiplication
/// [doc](https://developer.arm.com/documentation/dui0801/g/A64-SIMD-Vector-Instructions/FMUL--vector-)
#[target_feature(enable = "fp16")]
#[inline]
pub unsafe fn vmulq_f16(a: float16x8_t, b: float16x8_t) -> float16x8_t {
    let result: float16x8_t;
    asm!(
            "fmul {0:v}.8h, {1:v}.8h, {2:v}.8h",
            out(vreg) result,
            in(vreg) a,
            in(vreg) b,
            options(pure, nomem, nostack));
    result
}

/// Floating point addition
/// [doc](https://developer.arm.com/documentation/dui0801/g/A64-SIMD-Vector-Instructions/FADD--vector-)
#[target_feature(enable = "fp16")]
#[inline]
pub unsafe fn vaddq_f16(a: float16x8_t, b: float16x8_t) -> float16x8_t {
    let result: float16x8_t;
    asm!(
            "fadd {0:v}.8h, {1:v}.8h, {2:v}.8h",
            out(vreg) result,
            in(vreg) a,
            in(vreg) b,
            options(pure, nomem, nostack));
    result
}

/// Fused multiply add [doc](https://developer.arm.com/documentation/dui0801/g/A64-SIMD-Vector-Instructions/FMLA--vector-)
#[target_feature(enable = "fp16")]
#[inline]
pub unsafe fn vfmaq_f16(mut a: float16x8_t, b: float16x8_t, c: float16x8_t) -> float16x8_t {
    asm!(
            "fmla {0:v}.8h, {1:v}.8h, {2:v}.8h",
            inout(vreg) a,
            in(vreg) b,
            in(vreg) c,
            options(pure, nomem, nostack));
    a
}

#[target_feature(enable = "fp16")]
#[inline]
pub unsafe fn vfmaq_laneq_f16<const LANE: i32>(
    mut a: float16x8_t,
    b: float16x8_t,
    c: float16x8_t,
) -> float16x8_t {
    match LANE {
        0 => asm!(
            "fmla {0:v}.8h, {1:v}.8h, {2:v}.h[0]",
            inout(vreg) a,
            in(vreg) b,
            in(vreg_low16) c,
            options(pure, nomem, nostack)),
        1 => asm!(
            "fmla {0:v}.8h, {1:v}.8h, {2:v}.h[1]",
            inout(vreg) a,
            in(vreg) b,
            in(vreg_low16) c,
            options(pure, nomem, nostack)),
        2 => asm!(
            "fmla {0:v}.8h, {1:v}.8h, {2:v}.h[2]",
            inout(vreg) a,
            in(vreg) b,
            in(vreg_low16) c,
            options(pure, nomem, nostack)),
        3 => asm!(
            "fmla {0:v}.8h, {1:v}.8h, {2:v}.h[3]",
            inout(vreg) a,
            in(vreg) b,
            in(vreg_low16) c,
            options(pure, nomem, nostack)),
        4 => asm!(
            "fmla {0:v}.8h, {1:v}.8h, {2:v}.h[4]",
            inout(vreg) a,
            in(vreg) b,
            in(vreg_low16) c,
            options(pure, nomem, nostack)),
        5 => asm!(
            "fmla {0:v}.8h, {1:v}.8h, {2:v}.h[5]",
            inout(vreg) a,
            in(vreg) b,
            in(vreg_low16) c,
            options(pure, nomem, nostack)),
        6 => asm!(
            "fmla {0:v}.8h, {1:v}.8h, {2:v}.h[6]",
            inout(vreg) a,
            in(vreg) b,
            in(vreg_low16) c,
            options(pure, nomem, nostack)),
        7 => asm!(
            "fmla {0:v}.8h, {1:v}.8h, {2:v}.h[7]",
            inout(vreg) a,
            in(vreg) b,
            in(vreg_low16) c,
            options(pure, nomem, nostack)),
        _ => unreachable!(),
    }
    a
}

#[derive(Copy, Clone, Debug)]
pub struct Neon {
    __private: (),
}

#[derive(Copy, Clone, Debug)]
pub struct NeonFp16 {
    __private: (),
}

#[derive(Copy, Clone, Debug)]
pub struct NeonFcma {
    __private: (),
}

impl Simd for Neon {
    #[inline]
    #[target_feature(enable = "neon")]
    unsafe fn vectorize<F: NullaryFnOnce>(f: F) -> F::Output {
        f.call()
    }
}

impl Simd for NeonFp16 {
    #[inline]
    #[target_feature(enable = "neon,fp16")]
    unsafe fn vectorize<F: NullaryFnOnce>(f: F) -> F::Output {
        f.call()
    }
}

#[cfg(feature = "f16")]
unsafe impl MixedSimd<f16, f16, f16, f32> for NeonFp16 {
    const SIMD_WIDTH: usize = 4;

    type LhsN = [f16; 4];
    type RhsN = [f16; 4];
    type DstN = [f16; 4];
    type AccN = [f32; 4];

    #[inline]
    fn try_new() -> Option<Self> {
        if crate::feature_detected!("neon") && crate::feature_detected!("fp16") {
            Some(Self { __private: () })
        } else {
            None
        }
    }

    #[inline(always)]
    fn mult(self, lhs: f32, rhs: f32) -> f32 {
        lhs * rhs
    }

    #[inline(always)]
    fn mult_add(self, lhs: f32, rhs: f32, acc: f32) -> f32 {
        unsafe { neon_fmaf(lhs, rhs, acc) }
    }

    #[inline(always)]
    fn from_lhs(self, lhs: f16) -> f32 {
        unsafe { f16_to_f32_fp16(cast(lhs)) }
    }

    #[inline(always)]
    fn from_rhs(self, rhs: f16) -> f32 {
        unsafe { f16_to_f32_fp16(cast(rhs)) }
    }

    #[inline(always)]
    fn from_dst(self, dst: f16) -> f32 {
        unsafe { f16_to_f32_fp16(cast(dst)) }
    }

    #[inline(always)]
    fn into_dst(self, acc: f32) -> f16 {
        unsafe { cast(f32_to_f16_fp16(acc)) }
    }

    #[inline(always)]
    fn simd_mult_add(self, lhs: Self::AccN, rhs: Self::AccN, acc: Self::AccN) -> Self::AccN {
        unsafe { transmute(vfmaq_f32(transmute(acc), transmute(lhs), transmute(rhs))) }
    }

    #[inline(always)]
    fn simd_from_lhs(self, lhs: Self::LhsN) -> Self::AccN {
        unsafe { f16x4_to_f32x4_fp16(&cast(lhs)) }
    }

    #[inline(always)]
    fn simd_from_rhs(self, rhs: Self::RhsN) -> Self::AccN {
        unsafe { f16x4_to_f32x4_fp16(&cast(rhs)) }
    }

    #[inline(always)]
    fn simd_splat(self, lhs: f32) -> Self::AccN {
        [lhs, lhs, lhs, lhs]
    }

    #[inline(always)]
    fn simd_from_dst(self, dst: Self::DstN) -> Self::AccN {
        unsafe { f16x4_to_f32x4_fp16(&cast(dst)) }
    }

    #[inline(always)]
    fn simd_into_dst(self, acc: Self::AccN) -> Self::DstN {
        unsafe { cast(f32x4_to_f16x4_fp16(&acc)) }
    }

    #[inline(always)]
    fn vectorize<F: NullaryFnOnce>(self, f: F) -> F::Output {
        #[inline]
        #[target_feature(enable = "neon,fp16")]
        unsafe fn implementation<F: NullaryFnOnce>(f: F) -> F::Output {
            f.call()
        }

        unsafe { implementation(f) }
    }

    #[inline(always)]
    fn add(self, lhs: f32, rhs: f32) -> f32 {
        lhs + rhs
    }

    #[inline(always)]
    fn simd_mul(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        unsafe { transmute(vmulq_f32(transmute(lhs), transmute(rhs))) }
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        unsafe { transmute(vaddq_f32(transmute(lhs), transmute(rhs))) }
    }
}

#[cfg(feature = "f16")]
unsafe impl MixedSimd<f16, f16, f16, f16> for NeonFp16 {
    const SIMD_WIDTH: usize = 8;

    type LhsN = [f16; 8];
    type RhsN = [f16; 8];
    type DstN = [f16; 8];
    type AccN = [f16; 8];

    #[inline]
    fn try_new() -> Option<Self> {
        if crate::feature_detected!("neon") && crate::feature_detected!("fp16") {
            Some(Self { __private: () })
        } else {
            None
        }
    }

    #[inline(always)]
    fn mult(self, lhs: f16, rhs: f16) -> f16 {
        unsafe { cast(multiply_f16_fp16(cast(lhs), cast(rhs))) }
    }

    #[inline(always)]
    fn mult_add(self, lhs: f16, rhs: f16, acc: f16) -> f16 {
        unsafe { cast(fmaq_f16(cast(acc), cast(lhs), cast(rhs))) }
    }

    #[inline(always)]
    fn from_lhs(self, lhs: f16) -> f16 {
        lhs
    }

    #[inline(always)]
    fn from_rhs(self, rhs: f16) -> f16 {
        rhs
    }

    #[inline(always)]
    fn from_dst(self, dst: f16) -> f16 {
        dst
    }

    #[inline(always)]
    fn into_dst(self, acc: f16) -> f16 {
        acc
    }

    #[inline(always)]
    fn simd_mult_add(self, lhs: Self::AccN, rhs: Self::AccN, acc: Self::AccN) -> Self::AccN {
        unsafe { transmute(vfmaq_f16(transmute(acc), transmute(lhs), transmute(rhs))) }
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
    fn simd_splat(self, lhs: f16) -> Self::AccN {
        [lhs, lhs, lhs, lhs, lhs, lhs, lhs, lhs]
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
        #[inline]
        #[target_feature(enable = "neon,fp16")]
        unsafe fn implementation<F: NullaryFnOnce>(f: F) -> F::Output {
            f.call()
        }

        unsafe { implementation(f) }
    }

    #[inline(always)]
    fn add(self, lhs: f16, rhs: f16) -> f16 {
        unsafe { cast(add_f16_fp16(cast(lhs), cast(rhs))) }
    }

    #[inline(always)]
    fn simd_mul(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        unsafe { transmute(vmulq_f16(transmute(lhs), transmute(rhs))) }
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        unsafe { transmute(vaddq_f16(transmute(lhs), transmute(rhs))) }
    }
}

#[cfg(feature = "f16")]
unsafe impl MixedSimd<f16, f16, f16, f32> for Neon {
    const SIMD_WIDTH: usize = 4;

    type LhsN = [f16; 4];
    type RhsN = [f16; 4];
    type DstN = [f16; 4];
    type AccN = [f32; 4];

    #[inline]
    fn try_new() -> Option<Self> {
        if crate::feature_detected!("neon") {
            Some(Self { __private: () })
        } else {
            None
        }
    }

    #[inline(always)]
    fn mult(self, lhs: f32, rhs: f32) -> f32 {
        lhs * rhs
    }

    #[inline(always)]
    fn mult_add(self, lhs: f32, rhs: f32, acc: f32) -> f32 {
        unsafe { neon_fmaf(lhs, rhs, acc) }
    }

    #[inline(always)]
    fn from_lhs(self, lhs: f16) -> f32 {
        lhs.into()
    }

    #[inline(always)]
    fn from_rhs(self, rhs: f16) -> f32 {
        rhs.into()
    }

    #[inline(always)]
    fn from_dst(self, dst: f16) -> f32 {
        dst.into()
    }

    #[inline(always)]
    fn into_dst(self, acc: f32) -> f16 {
        f16::from_f32(acc)
    }

    #[inline(always)]
    fn simd_mult_add(self, lhs: Self::AccN, rhs: Self::AccN, acc: Self::AccN) -> Self::AccN {
        unsafe { transmute(vfmaq_f32(transmute(acc), transmute(lhs), transmute(rhs))) }
    }

    #[inline(always)]
    fn simd_from_lhs(self, lhs: Self::LhsN) -> Self::AccN {
        [lhs[0].into(), lhs[1].into(), lhs[2].into(), lhs[3].into()]
    }

    #[inline(always)]
    fn simd_from_rhs(self, rhs: Self::RhsN) -> Self::AccN {
        [rhs[0].into(), rhs[1].into(), rhs[2].into(), rhs[3].into()]
    }

    #[inline(always)]
    fn simd_splat(self, lhs: f32) -> Self::AccN {
        [lhs, lhs, lhs, lhs]
    }

    #[inline(always)]
    fn simd_from_dst(self, dst: Self::DstN) -> Self::AccN {
        [dst[0].into(), dst[1].into(), dst[2].into(), dst[3].into()]
    }

    #[inline(always)]
    fn simd_into_dst(self, acc: Self::AccN) -> Self::DstN {
        [
            f16::from_f32(acc[0]),
            f16::from_f32(acc[1]),
            f16::from_f32(acc[2]),
            f16::from_f32(acc[3]),
        ]
    }

    #[inline(always)]
    fn vectorize<F: NullaryFnOnce>(self, f: F) -> F::Output {
        #[inline]
        #[target_feature(enable = "neon")]
        unsafe fn implementation<F: NullaryFnOnce>(f: F) -> F::Output {
            f.call()
        }

        unsafe { implementation(f) }
    }

    #[inline(always)]
    fn add(self, lhs: f32, rhs: f32) -> f32 {
        lhs + rhs
    }

    #[inline(always)]
    fn simd_mul(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        unsafe { transmute(vmulq_f32(transmute(lhs), transmute(rhs))) }
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        unsafe { transmute(vaddq_f32(transmute(lhs), transmute(rhs))) }
    }
}
