#[macro_export]
macro_rules! microkernel_cplx {
    ($([$target: tt])?, $unroll: tt, $name: ident, $mr_div_n: tt, $nr: tt) => {
        $(#[target_feature(enable = $target)])?
        // 0, 1, or 2 for generic alpha
        pub unsafe fn $name(
            m: usize,
            n: usize,
            k: usize,
            dst: *mut num_complex::Complex<T>,
            mut packed_lhs: *const num_complex::Complex<T>,
            mut packed_rhs: *const num_complex::Complex<T>,
            dst_cs: isize,
            dst_rs: isize,
            lhs_cs: isize,
            rhs_rs: isize,
            rhs_cs: isize,
            alpha: num_complex::Complex<T>,
            beta: num_complex::Complex<T>,
            alpha_status: u8,
            conj_dst: bool,
            conj_lhs: bool,
            conj_rhs: bool,
            mut next_lhs: *const num_complex::Complex<T>,
        ) {
            let mut accum_storage = [[splat(0.0); $mr_div_n]; $nr];
            let accum = accum_storage.as_mut_ptr() as *mut Pack;

            let conj_both_lhs_rhs = conj_lhs;
            let conj_rhs = conj_lhs != conj_rhs;

            let mut lhs_re_im = [::core::mem::MaybeUninit::<Pack>::uninit(); $mr_div_n];
            let mut lhs_im_re = [::core::mem::MaybeUninit::<Pack>::uninit(); $mr_div_n];
            let mut rhs_re = ::core::mem::MaybeUninit::<Pack>::uninit();
            let mut rhs_im = ::core::mem::MaybeUninit::<Pack>::uninit();

            #[derive(Copy, Clone)]
            struct KernelIter {
                packed_lhs: *const num_complex::Complex<T>,
                next_lhs: *const num_complex::Complex<T>,
                packed_rhs: *const num_complex::Complex<T>,
                lhs_cs: isize,
                rhs_rs: isize,
                rhs_cs: isize,
                accum: *mut Pack,
                lhs_re_im: *mut Pack,
                lhs_im_re: *mut Pack,
                rhs_re: *mut Pack,
                rhs_im: *mut Pack,
            }

            impl KernelIter {
                #[inline(always)]
                unsafe fn execute(self, iter: usize, conj_rhs: bool) {
                    let packed_lhs = self.packed_lhs.wrapping_offset(iter as isize * self.lhs_cs);
                    let packed_rhs = self.packed_rhs.wrapping_offset(iter as isize * self.rhs_rs);
                    let next_lhs = self.next_lhs.wrapping_offset(iter as isize * self.lhs_cs);

                    seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                        let tmp = *(packed_lhs.add(M_ITER * CPLX_N) as *const Pack);
                        *self.lhs_re_im.add(M_ITER) = tmp;
                        *self.lhs_im_re.add(M_ITER) = swap_re_im(tmp);
                    }});

                    seq_macro::seq!(N_ITER in 0..$nr {{
                        *self.rhs_re = splat((*packed_rhs.wrapping_offset(N_ITER * self.rhs_cs)).re);
                        *self.rhs_im = splat((*packed_rhs.wrapping_offset(N_ITER * self.rhs_cs)).im);

                        let accum = self.accum.add(N_ITER * $mr_div_n);
                        seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                            let accum = &mut *accum.add(M_ITER);
                            *accum = mul_add_cplx(
                                *self.lhs_re_im.add(M_ITER),
                                *self.lhs_im_re.add(M_ITER),
                                *self.rhs_re,
                                *self.rhs_im,
                                *accum,
                                conj_rhs,
                                );
                        }});
                    }});

                    let _ = next_lhs;
                }
            }

            let k_unroll = k / $unroll;
            let k_leftover = k % $unroll;

            loop {
                if conj_rhs {
                    let mut depth = k_unroll;
                    if depth != 0 {
                        loop {
                            let iter = KernelIter {
                                packed_lhs,
                                next_lhs,
                                packed_rhs,
                                lhs_cs,
                                rhs_rs,
                                rhs_cs,
                                accum,
                                lhs_re_im: lhs_re_im.as_mut_ptr() as _,
                                lhs_im_re: lhs_im_re.as_mut_ptr() as _,
                                rhs_re: &mut rhs_re as *mut _ as _,
                                rhs_im: &mut rhs_im as *mut _ as _,
                            };

                            seq_macro::seq!(UNROLL_ITER in 0..$unroll {{
                                iter.execute(UNROLL_ITER, true);
                            }});

                            packed_lhs = packed_lhs.wrapping_offset($unroll * lhs_cs);
                            packed_rhs = packed_rhs.wrapping_offset($unroll * rhs_rs);
                            next_lhs = next_lhs.wrapping_offset($unroll * lhs_cs);

                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    depth = k_leftover;
                    if depth != 0 {
                        loop {
                            KernelIter {
                                packed_lhs,
                                next_lhs,
                                packed_rhs,
                                lhs_cs,
                                rhs_rs,
                                rhs_cs,
                                accum,
                                lhs_re_im: lhs_re_im.as_mut_ptr() as _,
                                lhs_im_re: lhs_im_re.as_mut_ptr() as _,
                                rhs_re: &mut rhs_re as *mut _ as _,
                                rhs_im: &mut rhs_im as *mut _ as _,
                            }
                            .execute(0, true);

                            packed_lhs = packed_lhs.wrapping_offset(lhs_cs);
                            packed_rhs = packed_rhs.wrapping_offset(rhs_rs);
                            next_lhs = next_lhs.wrapping_offset(lhs_cs);

                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    break;
                } else {
                    let mut depth = k_unroll;
                    if depth != 0 {
                        loop {
                            let iter = KernelIter {
                                next_lhs,
                                packed_lhs,
                                packed_rhs,
                                lhs_cs,
                                rhs_rs,
                                rhs_cs,
                                accum,
                                lhs_re_im: lhs_re_im.as_mut_ptr() as _,
                                lhs_im_re: lhs_im_re.as_mut_ptr() as _,
                                rhs_re: &mut rhs_re as *mut _ as _,
                                rhs_im: &mut rhs_im as *mut _ as _,
                            };

                            seq_macro::seq!(UNROLL_ITER in 0..$unroll {{
                                iter.execute(UNROLL_ITER, false);
                            }});

                            packed_lhs = packed_lhs.wrapping_offset($unroll * lhs_cs);
                            packed_rhs = packed_rhs.wrapping_offset($unroll * rhs_rs);
                            next_lhs = next_lhs.wrapping_offset($unroll * lhs_cs);

                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    depth = k_leftover;
                    if depth != 0 {
                        loop {
                            KernelIter {
                                next_lhs,
                                packed_lhs,
                                packed_rhs,
                                lhs_cs,
                                rhs_rs,
                                rhs_cs,
                                accum,
                                lhs_re_im: lhs_re_im.as_mut_ptr() as _,
                                lhs_im_re: lhs_im_re.as_mut_ptr() as _,
                                rhs_re: &mut rhs_re as *mut _ as _,
                                rhs_im: &mut rhs_im as *mut _ as _,
                            }
                            .execute(0, false);

                            packed_lhs = packed_lhs.wrapping_offset(lhs_cs);
                            packed_rhs = packed_rhs.wrapping_offset(rhs_rs);
                            next_lhs = next_lhs.wrapping_offset(lhs_cs);

                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    break;
                }
            }

            if conj_both_lhs_rhs {
                seq_macro::seq!(N_ITER in 0..$nr {{
                    let accum = accum.add(N_ITER * $mr_div_n);
                    seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                        let accum = &mut *accum.add(M_ITER);
                        *accum = conj(*accum);
                    }});
                }});
            }

            if m == $mr_div_n * CPLX_N && n == $nr && dst_rs == 1 {
                let alpha_re = splat(alpha.re);
                let alpha_im = splat(alpha.im);
                let beta_re = splat(beta.re);
                let beta_im = splat(beta.im);

                if conj_dst {
                    if alpha_status == 2 {
                        seq_macro::seq!(N_ITER in 0..$nr {{
                            seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                                let dst = dst.offset(M_ITER * CPLX_N as isize + N_ITER * dst_cs) as *mut Pack;
                                let accum = *accum.offset(M_ITER + $mr_div_n * N_ITER);
                                *dst = add(
                                    mul_cplx(conj(*dst), swap_re_im(conj(*dst)), alpha_re, alpha_im),
                                    mul_cplx(accum, swap_re_im(accum), beta_re, beta_im),
                                    );
                            }});
                        }});
                    } else if alpha_status == 1 {
                        seq_macro::seq!(N_ITER in 0..$nr {{
                            seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                                let dst = dst.offset(M_ITER * CPLX_N as isize + N_ITER * dst_cs) as *mut Pack;
                                let accum = *accum.offset(M_ITER + $mr_div_n * N_ITER);
                                *dst = add(
                                    conj(*dst),
                                    mul_cplx(accum, swap_re_im(accum), beta_re, beta_im),
                                    );
                            }});
                        }});
                    } else {
                        seq_macro::seq!(N_ITER in 0..$nr {{
                            seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                                let dst = dst.offset(M_ITER * CPLX_N as isize + N_ITER * dst_cs) as *mut Pack;
                                let accum = *accum.offset(M_ITER + $mr_div_n * N_ITER);
                                *dst = mul_cplx(accum, swap_re_im(accum), beta_re, beta_im);
                            }});
                        }});
                    }
                } else {
                    if alpha_status == 2 {
                        seq_macro::seq!(N_ITER in 0..$nr {{
                            seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                                let dst = dst.offset(M_ITER * CPLX_N as isize + N_ITER * dst_cs) as *mut Pack;
                                let accum = *accum.offset(M_ITER + $mr_div_n * N_ITER);
                                *dst = add(
                                    mul_cplx(*dst, swap_re_im(*dst), alpha_re, alpha_im),
                                    mul_cplx(accum, swap_re_im(accum), beta_re, beta_im),
                                );
                            }});
                        }});
                    } else if alpha_status == 1 {
                        seq_macro::seq!(N_ITER in 0..$nr {{
                            seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                                let dst = dst.offset(M_ITER * CPLX_N as isize + N_ITER * dst_cs) as *mut Pack;
                                let accum = *accum.offset(M_ITER + $mr_div_n * N_ITER);
                                *dst = add(
                                    *dst,
                                    mul_cplx(accum, swap_re_im(accum), beta_re, beta_im),
                                );
                            }});
                        }});
                    } else {
                        seq_macro::seq!(N_ITER in 0..$nr {{
                            seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                                let dst = dst.offset(M_ITER * CPLX_N as isize + N_ITER * dst_cs) as *mut Pack;
                                let accum = *accum.offset(M_ITER + $mr_div_n * N_ITER);
                                *dst = mul_cplx(accum, swap_re_im(accum), beta_re, beta_im);
                            }});
                        }});
                    }
                }
            } else {
                let src = accum_storage; // write to stack
                let src = src.as_ptr() as *const num_complex::Complex<T>;

                if conj_dst {
                    if alpha_status == 2 {
                        for j in 0..n {
                            let dst_j = dst.offset(dst_cs * j as isize);
                            let src_j = src.add(j * $mr_div_n * CPLX_N);

                            for i in 0..m {
                                let dst_ij = dst_j.offset(dst_rs * i as isize);
                                let src_ij = src_j.add(i);

                                *dst_ij = alpha * (*dst_ij).conj() + beta * *src_ij;
                            }
                        }
                    } else if alpha_status == 1 {
                        for j in 0..n {
                            let dst_j = dst.offset(dst_cs * j as isize);
                            let src_j = src.add(j * $mr_div_n * CPLX_N);

                            for i in 0..m {
                                let dst_ij = dst_j.offset(dst_rs * i as isize);
                                let src_ij = src_j.add(i);

                                *dst_ij = (*dst_ij).conj() + beta * *src_ij;
                            }
                        }
                    } else {
                        for j in 0..n {
                            let dst_j = dst.offset(dst_cs * j as isize);
                            let src_j = src.add(j * $mr_div_n * CPLX_N);

                            for i in 0..m {
                                let dst_ij = dst_j.offset(dst_rs * i as isize);
                                let src_ij = src_j.add(i);

                                *dst_ij = beta * *src_ij;
                            }
                        }
                    }
                } else {
                    if alpha_status == 2 {
                        for j in 0..n {
                            let dst_j = dst.offset(dst_cs * j as isize);
                            let src_j = src.add(j * $mr_div_n * CPLX_N);

                            for i in 0..m {
                                let dst_ij = dst_j.offset(dst_rs * i as isize);
                                let src_ij = src_j.add(i);

                                *dst_ij = alpha * *dst_ij + beta * *src_ij;
                            }
                        }
                    } else if alpha_status == 1 {
                        for j in 0..n {
                            let dst_j = dst.offset(dst_cs * j as isize);
                            let src_j = src.add(j * $mr_div_n * CPLX_N);

                            for i in 0..m {
                                let dst_ij = dst_j.offset(dst_rs * i as isize);
                                let src_ij = src_j.add(i);

                                *dst_ij = *dst_ij + beta * *src_ij;
                            }
                        }
                    } else {
                        for j in 0..n {
                            let dst_j = dst.offset(dst_cs * j as isize);
                            let src_j = src.add(j * $mr_div_n * CPLX_N);

                            for i in 0..m {
                                let dst_ij = dst_j.offset(dst_rs * i as isize);
                                let src_ij = src_j.add(i);

                                *dst_ij = beta * *src_ij;
                            }
                        }
                    }
                }
            }
        }
    };
}

#[macro_export]
macro_rules! microkernel_cplx_packed {
    ($([$target: tt])?, $unroll: tt, $name: ident, $mr_div_n: tt, $nr: tt) => {
        $(#[target_feature(enable = $target)])?
        // 0, 1, or 2 for generic alpha
        pub unsafe fn $name(
            m: usize,
            n: usize,
            k: usize,
            dst: *mut T,
            mut packed_lhs: *const T,
            mut packed_rhs: *const T,
            dst_cs: isize,
            dst_rs: isize,
            lhs_cs: isize,
            rhs_rs: isize,
            rhs_cs: isize,
            alpha: T,
            beta: T,
            alpha_status: u8,
            conj_dst: bool,
            conj_lhs: bool,
            conj_rhs: bool,
            mut next_lhs: *const T,
        ) {
            let mut accum_storage = [[core::mem::zeroed::<Pack>(); $mr_div_n]; $nr];
            let accum = accum_storage.as_mut_ptr() as *mut Pack;

            let conj_both_lhs_rhs = conj_lhs;
            let conj_rhs = conj_lhs != conj_rhs;

            let mut lhs = [::core::mem::MaybeUninit::<Pack>::uninit(); $mr_div_n];
            let mut rhs = ::core::mem::MaybeUninit::<Pack>::uninit();

            #[derive(Copy, Clone)]
            struct KernelIter {
                packed_lhs: *const T,
                next_lhs: *const T,
                packed_rhs: *const T,
                lhs_cs: isize,
                rhs_rs: isize,
                rhs_cs: isize,
                accum: *mut Pack,
                lhs: *mut Pack,
                rhs: *mut Pack,
            }

            impl KernelIter {
                #[inline(always)]
                unsafe fn execute(self, iter: usize, conj_rhs: bool) {
                    let packed_lhs = self.packed_lhs.wrapping_offset(iter as isize * self.lhs_cs);
                    let packed_rhs = self.packed_rhs.wrapping_offset(iter as isize * self.rhs_rs);
                    let next_lhs = self.next_lhs.wrapping_offset(iter as isize * self.lhs_cs);

                    seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                        *self.lhs.add(M_ITER) = *(packed_lhs.add(M_ITER * N) as *const Pack);
                    }});

                    seq_macro::seq!(N_ITER in 0..$nr {{
                        *self.rhs = splat(*packed_rhs.wrapping_offset(N_ITER * self.rhs_cs));

                        let accum = self.accum.add(N_ITER * $mr_div_n);
                        seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                            let accum = &mut *accum.add(M_ITER);
                            *accum = mul_add_cplx(
                                *self.lhs.add(M_ITER),
                                *self.rhs,
                                *accum,
                                conj_rhs,
                            );
                        }});
                    }});

                    let _ = next_lhs;
                }
            }

            let k_unroll = k / $unroll;
            let k_leftover = k % $unroll;

            loop {
                if conj_rhs {
                    let mut depth = k_unroll;
                    if depth != 0 {
                        loop {
                            let iter = KernelIter {
                                packed_lhs,
                                next_lhs,
                                packed_rhs,
                                lhs_cs,
                                rhs_rs,
                                rhs_cs,
                                accum,
                                lhs: lhs.as_mut_ptr() as _,
                                rhs: &mut rhs as *mut _ as _,
                            };

                            seq_macro::seq!(UNROLL_ITER in 0..$unroll {{
                                iter.execute(UNROLL_ITER, true);
                            }});

                            packed_lhs = packed_lhs.wrapping_offset($unroll * lhs_cs);
                            packed_rhs = packed_rhs.wrapping_offset($unroll * rhs_rs);
                            next_lhs = next_lhs.wrapping_offset($unroll * lhs_cs);

                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    depth = k_leftover;
                    if depth != 0 {
                        loop {
                            KernelIter {
                                packed_lhs,
                                next_lhs,
                                packed_rhs,
                                lhs_cs,
                                rhs_rs,
                                rhs_cs,
                                accum,
                                lhs: lhs.as_mut_ptr() as _,
                                rhs: &mut rhs as *mut _ as _,
                            }
                            .execute(0, true);

                            packed_lhs = packed_lhs.wrapping_offset(lhs_cs);
                            packed_rhs = packed_rhs.wrapping_offset(rhs_rs);
                            next_lhs = next_lhs.wrapping_offset(lhs_cs);

                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    break;
                } else {
                    let mut depth = k_unroll;
                    if depth != 0 {
                        loop {
                            let iter = KernelIter {
                                next_lhs,
                                packed_lhs,
                                packed_rhs,
                                lhs_cs,
                                rhs_rs,
                                rhs_cs,
                                accum,
                                lhs: lhs.as_mut_ptr() as _,
                                rhs: &mut rhs as *mut _ as _,
                            };

                            seq_macro::seq!(UNROLL_ITER in 0..$unroll {{
                                iter.execute(UNROLL_ITER, false);
                            }});

                            packed_lhs = packed_lhs.wrapping_offset($unroll * lhs_cs);
                            packed_rhs = packed_rhs.wrapping_offset($unroll * rhs_rs);
                            next_lhs = next_lhs.wrapping_offset($unroll * lhs_cs);

                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    depth = k_leftover;
                    if depth != 0 {
                        loop {
                            KernelIter {
                                next_lhs,
                                packed_lhs,
                                packed_rhs,
                                lhs_cs,
                                rhs_rs,
                                rhs_cs,
                                accum,
                                lhs: lhs.as_mut_ptr() as _,
                                rhs: &mut rhs as *mut _ as _,
                            }
                            .execute(0, false);

                            packed_lhs = packed_lhs.wrapping_offset(lhs_cs);
                            packed_rhs = packed_rhs.wrapping_offset(rhs_rs);
                            next_lhs = next_lhs.wrapping_offset(lhs_cs);

                            depth -= 1;
                            if depth == 0 {
                                break;
                            }
                        }
                    }
                    break;
                }
            }

            if conj_both_lhs_rhs {
                seq_macro::seq!(N_ITER in 0..$nr {{
                    let accum = accum.add(N_ITER * $mr_div_n);
                    seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                        let accum = &mut *accum.add(M_ITER);
                        *accum = conj(*accum);
                    }});
                }});
            }

            if m == $mr_div_n * N && n == $nr && dst_rs == 1 {
                let alpha = splat(alpha);
                let beta = splat(beta);

                if conj_dst {
                    if alpha_status == 2 {
                        seq_macro::seq!(N_ITER in 0..$nr {{
                            seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                                let dst = dst.offset(M_ITER * N as isize + N_ITER * dst_cs) as *mut Pack;
                                let accum = *accum.offset(M_ITER + $mr_div_n * N_ITER);
                                *dst = add(
                                    mul_cplx(conj(*dst), alpha),
                                    mul_cplx(accum, beta),
                                );
                            }});
                        }});
                    } else if alpha_status == 1 {
                        seq_macro::seq!(N_ITER in 0..$nr {{
                            seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                                let dst = dst.offset(M_ITER * N as isize + N_ITER * dst_cs) as *mut Pack;
                                let accum = *accum.offset(M_ITER + $mr_div_n * N_ITER);
                                *dst = add(
                                    conj(*dst),
                                    mul_cplx(accum, beta),
                                    );
                            }});
                        }});
                    } else {
                        seq_macro::seq!(N_ITER in 0..$nr {{
                            seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                                let dst = dst.offset(M_ITER * N as isize + N_ITER * dst_cs) as *mut Pack;
                                let accum = *accum.offset(M_ITER + $mr_div_n * N_ITER);
                                *dst = mul_cplx(accum, beta);
                            }});
                        }});
                    }
                } else {
                    if alpha_status == 2 {
                        seq_macro::seq!(N_ITER in 0..$nr {{
                            seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                                let dst = dst.offset(M_ITER * N as isize + N_ITER * dst_cs) as *mut Pack;
                                let accum = *accum.offset(M_ITER + $mr_div_n * N_ITER);
                                *dst = add(
                                    mul_cplx(*dst, alpha),
                                    mul_cplx(accum, beta),
                                );
                            }});
                        }});
                    } else if alpha_status == 1 {
                        seq_macro::seq!(N_ITER in 0..$nr {{
                            seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                                let dst = dst.offset(M_ITER * N as isize + N_ITER * dst_cs) as *mut Pack;
                                let accum = *accum.offset(M_ITER + $mr_div_n * N_ITER);
                                *dst = add(
                                    *dst,
                                    mul_cplx(accum, beta),
                                );
                            }});
                        }});
                    } else {
                        seq_macro::seq!(N_ITER in 0..$nr {{
                            seq_macro::seq!(M_ITER in 0..$mr_div_n {{
                                let dst = dst.offset(M_ITER * N as isize + N_ITER * dst_cs) as *mut Pack;
                                let accum = *accum.offset(M_ITER + $mr_div_n * N_ITER);
                                *dst = mul_cplx(accum, beta);
                            }});
                        }});
                    }
                }
            } else {
                let src = accum_storage; // write to stack
                let src = src.as_ptr() as *const T;

                if conj_dst {
                    if alpha_status == 2 {
                        for j in 0..n {
                            let dst_j = dst.offset(dst_cs * j as isize);
                            let src_j = src.add(j * $mr_div_n * N);

                            for i in 0..m {
                                let dst_ij = dst_j.offset(dst_rs * i as isize);
                                let src_ij = src_j.add(i);

                                *dst_ij = alpha * (*dst_ij).conj() + beta * *src_ij;
                            }
                        }
                    } else if alpha_status == 1 {
                        for j in 0..n {
                            let dst_j = dst.offset(dst_cs * j as isize);
                            let src_j = src.add(j * $mr_div_n * N);

                            for i in 0..m {
                                let dst_ij = dst_j.offset(dst_rs * i as isize);
                                let src_ij = src_j.add(i);

                                *dst_ij = (*dst_ij).conj() + beta * *src_ij;
                            }
                        }
                    } else {
                        for j in 0..n {
                            let dst_j = dst.offset(dst_cs * j as isize);
                            let src_j = src.add(j * $mr_div_n * N);

                            for i in 0..m {
                                let dst_ij = dst_j.offset(dst_rs * i as isize);
                                let src_ij = src_j.add(i);

                                *dst_ij = beta * *src_ij;
                            }
                        }
                    }
                } else {
                    if alpha_status == 2 {
                        for j in 0..n {
                            let dst_j = dst.offset(dst_cs * j as isize);
                            let src_j = src.add(j * $mr_div_n * N);

                            for i in 0..m {
                                let dst_ij = dst_j.offset(dst_rs * i as isize);
                                let src_ij = src_j.add(i);

                                *dst_ij = alpha * *dst_ij + beta * *src_ij;
                            }
                        }
                    } else if alpha_status == 1 {
                        for j in 0..n {
                            let dst_j = dst.offset(dst_cs * j as isize);
                            let src_j = src.add(j * $mr_div_n * N);

                            for i in 0..m {
                                let dst_ij = dst_j.offset(dst_rs * i as isize);
                                let src_ij = src_j.add(i);

                                *dst_ij = *dst_ij + beta * *src_ij;
                            }
                        }
                    } else {
                        for j in 0..n {
                            let dst_j = dst.offset(dst_cs * j as isize);
                            let src_j = src.add(j * $mr_div_n * N);

                            for i in 0..m {
                                let dst_ij = dst_j.offset(dst_rs * i as isize);
                                let src_ij = src_j.add(i);

                                *dst_ij = beta * *src_ij;
                            }
                        }
                    }
                }
            }
        }
    };
}
