pub use bytemuck::Pod;
#[cfg(feature = "f16")]
use half::f16;
pub use pulp::{cast, NullaryFnOnce};

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
pub use x86::*;

#[cfg(target_arch = "aarch64")]
pub use aarch64::*;

use crate::gemm::{c32, c64};

pub trait Simd: Copy + Send + Sync + 'static {
    unsafe fn vectorize<F: NullaryFnOnce>(f: F) -> F::Output;
}

#[derive(Copy, Clone, Debug)]
pub struct Scalar;

impl Simd for Scalar {
    #[inline(always)]
    unsafe fn vectorize<F: NullaryFnOnce>(f: F) -> F::Output {
        f.call()
    }
}

#[cfg(feature = "f16")]
unsafe impl MixedSimd<f16, f16, f16, f32> for Scalar {
    const SIMD_WIDTH: usize = 1;

    type LhsN = f16;
    type RhsN = f16;
    type DstN = f16;
    type AccN = f32;

    #[inline]
    fn try_new() -> Option<Self> {
        Some(Self)
    }

    #[inline(always)]
    fn add(self, lhs: f32, rhs: f32) -> f32 {
        lhs + rhs
    }

    #[inline(always)]
    fn mult(self, lhs: f32, rhs: f32) -> f32 {
        lhs * rhs
    }

    #[inline(always)]
    fn mult_add(self, lhs: f32, rhs: f32, acc: f32) -> f32 {
        lhs * rhs + acc
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
        lhs * rhs + acc
    }

    #[inline(always)]
    fn simd_from_lhs(self, lhs: Self::LhsN) -> Self::AccN {
        lhs.into()
    }

    #[inline(always)]
    fn simd_from_rhs(self, rhs: Self::RhsN) -> Self::AccN {
        rhs.into()
    }

    #[inline(always)]
    fn simd_splat(self, lhs: f32) -> Self::AccN {
        lhs
    }

    #[inline(always)]
    fn simd_from_dst(self, dst: Self::DstN) -> Self::AccN {
        dst.into()
    }

    #[inline(always)]
    fn simd_into_dst(self, acc: Self::AccN) -> Self::DstN {
        f16::from_f32(acc)
    }

    #[inline(always)]
    fn vectorize<F: NullaryFnOnce>(self, f: F) -> F::Output {
        f.call()
    }

    #[inline(always)]
    fn simd_mul(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        lhs * rhs
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        lhs + rhs
    }
}

unsafe impl MixedSimd<f32, f32, f32, f32> for Scalar {
    const SIMD_WIDTH: usize = 1;

    type LhsN = f32;
    type RhsN = f32;
    type DstN = f32;
    type AccN = f32;

    #[inline]
    fn try_new() -> Option<Self> {
        Some(Self)
    }

    #[inline(always)]
    fn mult(self, lhs: f32, rhs: f32) -> f32 {
        lhs * rhs
    }

    #[inline(always)]
    fn mult_add(self, lhs: f32, rhs: f32, acc: f32) -> f32 {
        lhs * rhs + acc
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
        lhs * rhs + acc
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
        lhs
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
        f.call()
    }

    #[inline(always)]
    fn add(self, lhs: f32, rhs: f32) -> f32 {
        lhs + rhs
    }

    #[inline(always)]
    fn simd_mul(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        lhs * rhs
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        lhs + rhs
    }
}

unsafe impl MixedSimd<f64, f64, f64, f64> for Scalar {
    const SIMD_WIDTH: usize = 1;

    type LhsN = f64;
    type RhsN = f64;
    type DstN = f64;
    type AccN = f64;

    #[inline]
    fn try_new() -> Option<Self> {
        Some(Self)
    }

    #[inline(always)]
    fn mult(self, lhs: f64, rhs: f64) -> f64 {
        lhs * rhs
    }

    #[inline(always)]
    fn mult_add(self, lhs: f64, rhs: f64, acc: f64) -> f64 {
        lhs * rhs + acc
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
        lhs * rhs + acc
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
        lhs
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
        f.call()
    }

    #[inline(always)]
    fn add(self, lhs: f64, rhs: f64) -> f64 {
        lhs + rhs
    }

    #[inline(always)]
    fn simd_mul(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        lhs * rhs
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        lhs + rhs
    }
}

unsafe impl MixedSimd<c32, c32, c32, c32> for Scalar {
    const SIMD_WIDTH: usize = 1;

    type LhsN = c32;
    type RhsN = c32;
    type DstN = c32;
    type AccN = c32;

    #[inline]
    fn try_new() -> Option<Self> {
        Some(Self)
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
        lhs * rhs + acc
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
        lhs
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
        f.call()
    }

    #[inline(always)]
    fn add(self, lhs: c32, rhs: c32) -> c32 {
        lhs + rhs
    }

    #[inline(always)]
    fn simd_mul(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        lhs * rhs
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        lhs + rhs
    }
}

unsafe impl MixedSimd<c64, c64, c64, c64> for Scalar {
    const SIMD_WIDTH: usize = 1;

    type LhsN = c64;
    type RhsN = c64;
    type DstN = c64;
    type AccN = c64;

    #[inline]
    fn try_new() -> Option<Self> {
        Some(Self)
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
        lhs * rhs + acc
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
        lhs
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
        f.call()
    }

    #[inline(always)]
    fn add(self, lhs: c64, rhs: c64) -> c64 {
        lhs + rhs
    }

    #[inline(always)]
    fn simd_mul(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        lhs * rhs
    }

    #[inline(always)]
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN {
        lhs + rhs
    }
}

#[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
mod x86;

#[cfg(target_arch = "aarch64")]
pub mod aarch64;

pub trait Boilerplate: Copy + Send + Sync + core::fmt::Debug + 'static + PartialEq {}
impl<T: Copy + Send + Sync + core::fmt::Debug + PartialEq + 'static> Boilerplate for T {}

pub unsafe trait MixedSimd<Lhs, Rhs, Dst, Acc>: Simd {
    const SIMD_WIDTH: usize;

    type LhsN: Boilerplate;
    type RhsN: Boilerplate;
    type DstN: Boilerplate;
    type AccN: Boilerplate;

    fn try_new() -> Option<Self>;

    fn vectorize<F: NullaryFnOnce>(self, f: F) -> F::Output;

    fn add(self, lhs: Acc, rhs: Acc) -> Acc;
    fn mult(self, lhs: Acc, rhs: Acc) -> Acc;
    fn mult_add(self, lhs: Acc, rhs: Acc, acc: Acc) -> Acc;
    fn from_lhs(self, lhs: Lhs) -> Acc;
    fn from_rhs(self, rhs: Rhs) -> Acc;
    fn from_dst(self, dst: Dst) -> Acc;
    fn into_dst(self, acc: Acc) -> Dst;

    fn simd_mul(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN;
    fn simd_add(self, lhs: Self::AccN, rhs: Self::AccN) -> Self::AccN;
    fn simd_mult_add(self, lhs: Self::AccN, rhs: Self::AccN, acc: Self::AccN) -> Self::AccN;
    fn simd_from_lhs(self, lhs: Self::LhsN) -> Self::AccN;
    fn simd_from_rhs(self, rhs: Self::RhsN) -> Self::AccN;
    fn simd_splat(self, lhs: Acc) -> Self::AccN;

    fn simd_from_dst(self, dst: Self::DstN) -> Self::AccN;
    fn simd_into_dst(self, acc: Self::AccN) -> Self::DstN;
}
