#[macro_export]
macro_rules! __inject_mod {
    ($module: ident, $ty: ident, $N: expr, $simd: ident, $requires_packed_rhs: expr) => {
        mod $module {
            use super::*;
            use crate::gemm_common::simd::MixedSimd;
            use crate::microkernel::$module::$ty::*;
            const N: usize = $N;

            #[inline(never)]
            pub unsafe fn gemm_basic(
                m: usize,
                n: usize,
                k: usize,
                dst: *mut $ty,
                dst_cs: isize,
                dst_rs: isize,
                read_dst: bool,
                lhs: *const $ty,
                lhs_cs: isize,
                lhs_rs: isize,
                rhs: *const $ty,
                rhs_cs: isize,
                rhs_rs: isize,
                alpha: $ty,
                beta: $ty,
                conj_dst: bool,
                conj_lhs: bool,
                conj_rhs: bool,
                parallelism: $crate::Parallelism,
            ) {
                $crate::gemm::gemm_basic_generic::<
                    _,
                    $ty,
                    N,
                    { MR_DIV_N * N },
                    NR,
                    MR_DIV_N,
                    H_M,
                    H_N,
                >(
                    <$crate::simd::$simd as MixedSimd<$ty, $ty, $ty, $ty>>::try_new().unwrap(),
                    m,
                    n,
                    k,
                    dst,
                    dst_cs,
                    dst_rs,
                    read_dst,
                    lhs,
                    lhs_cs,
                    lhs_rs,
                    rhs,
                    rhs_cs,
                    rhs_rs,
                    alpha,
                    beta,
                    conj_dst,
                    conj_lhs,
                    conj_rhs,
                    |a, b, c| a * b + c,
                    &UKR,
                    &H_UKR,
                    $requires_packed_rhs,
                    parallelism,
                );
            }
        }
    };
}

#[macro_export]
macro_rules! __inject_mod_cplx {
    ($module: ident, $ty: ident, $N: expr, $simd: ident) => {
        paste::paste! {
            mod [<$module _cplx>] {
                use super::*;
                use crate::microkernel::$module::$ty::*;
                use crate::gemm_common::simd::MixedSimd;
                const N: usize = $N;

                #[inline(never)]
                pub unsafe fn gemm_basic_cplx(
                    m: usize,
                    n: usize,
                    k: usize,
                    dst: *mut num_complex::Complex<T>,
                    dst_cs: isize,
                    dst_rs: isize,
                    read_dst: bool,
                    lhs: *const num_complex::Complex<T>,
                    lhs_cs: isize,
                    lhs_rs: isize,
                    rhs: *const num_complex::Complex<T>,
                    rhs_cs: isize,
                    rhs_rs: isize,
                    alpha: num_complex::Complex<T>,
                    beta: num_complex::Complex<T>,
                    conj_dst: bool,
                    conj_lhs: bool,
                    conj_rhs: bool,
                    parallelism: $crate::Parallelism,
                    ) {
                    $crate::gemm::gemm_basic_generic::<_, _, N, { CPLX_MR_DIV_N * N }, CPLX_NR, CPLX_MR_DIV_N, H_CPLX_M, H_CPLX_N>(
                        <$crate::simd::$simd as MixedSimd<T, T, T, T>>::try_new().unwrap(),
                        m,
                        n,
                        k,
                        dst,
                        dst_cs,
                        dst_rs,
                        read_dst,
                        lhs,
                        lhs_cs,
                        lhs_rs,
                        rhs,
                        rhs_cs,
                        rhs_rs,
                        alpha,
                        beta,
                        conj_dst,
                        conj_lhs,
                        conj_rhs,
                        |a, b, c| a * b + c,
                        &CPLX_UKR,
                        &H_CPLX_UKR,
                        false,
                        parallelism,
                        );
                }
            }
        }
    };
}

#[macro_export]
macro_rules! gemm_def {
    ($ty: tt, $multiplier: expr) => {
        type GemmTy = unsafe fn(
            usize,
            usize,
            usize,
            *mut T,
            isize,
            isize,
            bool,
            *const T,
            isize,
            isize,
            *const T,
            isize,
            isize,
            T,
            T,
            bool,
            bool,
            bool,
            $crate::Parallelism,
        );

        #[inline]
        fn init_gemm_fn() -> GemmTy {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            {
                #[cfg(feature = "x86-v4")]
                if $crate::feature_detected!("avx512f") {
                    return avx512f::gemm_basic;
                }
                if $crate::feature_detected!("fma") {
                    fma::gemm_basic
                } else {
                    scalar::gemm_basic
                }
            }

            #[cfg(target_arch = "aarch64")]
            {
                if $crate::feature_detected!("neon") {
                    #[cfg(feature = "experimental-apple-amx")]
                    if $crate::cache::HasAmx::get() {
                        return amx::gemm_basic;
                    }
                    neon::gemm_basic
                } else {
                    scalar::gemm_basic
                }
            }

            #[cfg(target_arch = "wasm32")]
            {
                if $crate::feature_detected!("simd128") {
                    simd128::gemm_basic
                } else {
                    scalar::gemm_basic
                }
            }

            #[cfg(not(any(
                target_arch = "x86",
                target_arch = "x86_64",
                target_arch = "aarch64",
                target_arch = "wasm32",
            )))]
            {
                scalar::gemm_basic
            }
        }

        static GEMM_PTR: ::core::sync::atomic::AtomicPtr<()> =
            ::core::sync::atomic::AtomicPtr::new(::core::ptr::null_mut());

        #[inline(never)]
        fn init_gemm_ptr() -> GemmTy {
            let gemm_fn = init_gemm_fn();
            GEMM_PTR.store(gemm_fn as *mut (), ::core::sync::atomic::Ordering::Relaxed);
            gemm_fn
        }

        #[inline(always)]
        pub fn get_gemm_fn() -> GemmTy {
            let mut gemm_fn = GEMM_PTR.load(::core::sync::atomic::Ordering::Relaxed);
            if gemm_fn.is_null() {
                gemm_fn = init_gemm_ptr() as *mut ();
            }
            unsafe { ::core::mem::transmute(gemm_fn) }
        }

        $crate::__inject_mod!(scalar, $ty, 1, Scalar, false);

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        $crate::__inject_mod!(fma, $ty, 4 * $multiplier, V3, false);
        #[cfg(all(feature = "x86-v4", any(target_arch = "x86", target_arch = "x86_64")))]
        $crate::__inject_mod!(avx512f, $ty, 8 * $multiplier, V4, false);

        #[cfg(target_arch = "aarch64")]
        $crate::__inject_mod!(neon, $ty, 2 * $multiplier, Scalar, false);
        #[cfg(target_arch = "aarch64")]
        #[cfg(feature = "experimental-apple-amx")]
        $crate::__inject_mod!(amx, $ty, 8 * $multiplier, Scalar, true);

        #[cfg(target_arch = "wasm32")]
        $crate::__inject_mod!(simd128, $ty, 2 * $multiplier, Scalar, false);
    };
}

#[macro_export]
macro_rules! gemm_cplx_def {
    ($ty: tt, $cplx_ty: tt, $multiplier: expr) => {
        type GemmCplxTy = unsafe fn(
            usize,
            usize,
            usize,
            *mut num_complex::Complex<T>,
            isize,
            isize,
            bool,
            *const num_complex::Complex<T>,
            isize,
            isize,
            *const num_complex::Complex<T>,
            isize,
            isize,
            num_complex::Complex<T>,
            num_complex::Complex<T>,
            bool,
            bool,
            bool,
            $crate::Parallelism,
        );

        fn init_gemm_cplx_fn() -> GemmCplxTy {
            #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
            {
                #[cfg(feature = "x86-v4")]
                if $crate::feature_detected!("avx512f") {
                    return avx512f_cplx::gemm_basic_cplx;
                }
                if $crate::feature_detected!("fma") {
                    return fma_cplx::gemm_basic_cplx;
                }
            }

            #[cfg(target_arch = "aarch64")]
            {
                #[cfg(target_arch = "aarch64")]
                if $crate::feature_detected!("neon") && $crate::feature_detected!("fcma") {
                    return neonfcma::gemm_basic;
                }
            }

            scalar_cplx::gemm_basic_cplx
        }

        static GEMM_PTR: ::core::sync::atomic::AtomicPtr<()> =
            ::core::sync::atomic::AtomicPtr::new(::core::ptr::null_mut());

        #[inline(never)]
        fn init_gemm_ptr() -> GemmCplxTy {
            let gemm_fn = init_gemm_cplx_fn();
            GEMM_PTR.store(gemm_fn as *mut (), ::core::sync::atomic::Ordering::Relaxed);
            gemm_fn
        }

        #[inline(always)]
        pub fn get_gemm_fn() -> GemmCplxTy {
            let mut gemm_fn = GEMM_PTR.load(::core::sync::atomic::Ordering::Relaxed);
            if gemm_fn.is_null() {
                gemm_fn = init_gemm_ptr() as *mut ();
            }
            unsafe { ::core::mem::transmute(gemm_fn) }
        }

        $crate::__inject_mod_cplx!(scalar, $ty, 1, Scalar);

        #[cfg(target_arch = "aarch64")]
        $crate::__inject_mod!(neonfcma, $cplx_ty, 1 * $multiplier, Scalar, false);

        #[cfg(any(target_arch = "x86", target_arch = "x86_64"))]
        $crate::__inject_mod_cplx!(fma, $ty, 2 * $multiplier, V3);
        #[cfg(all(feature = "x86-v4", any(target_arch = "x86", target_arch = "x86_64")))]
        $crate::__inject_mod_cplx!(avx512f, $ty, 4 * $multiplier, V4);
    };
}
