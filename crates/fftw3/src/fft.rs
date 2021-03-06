use ndarray::{ArrayViewMut, Ix2, Ix3, IxDyn};
use num_complex::Complex;
use std;
// TODO: Replace with AtomicPtr or Arc<Mutex<>>
use crate::Float;
#[cfg(not(feature = "single"))]
use fftw3_ffi::fftw_plan_dft_2d;
#[cfg(feature = "single")]
use fftw3_ffi::fftwf_plan_dft_2d as fftw_plan_dft_2d;
use std::ptr::NonNull;

#[cfg(not(feature = "single"))]
use fftw3_ffi::fftw_plan_dft_3d;
#[cfg(feature = "single")]
use fftw3_ffi::fftwf_plan_dft_3d as fftw_plan_dft_3d;

#[cfg(not(feature = "single"))]
use fftw3_ffi::fftw_execute;
#[cfg(feature = "single")]
use fftw3_ffi::fftwf_execute as fftw_execute;

#[cfg(not(feature = "single"))]
use fftw3_ffi::fftw_execute_dft;
#[cfg(feature = "single")]
use fftw3_ffi::fftwf_execute_dft as fftw_execute_dft;

#[cfg(feature = "single")]
pub type FFTWComplex = ::fftw3_ffi::fftwf_complex;
#[cfg(not(feature = "single"))]
pub type FFTWComplex = ::fftw3_ffi::fftw_complex;

/// Wrapper to manage the state of an FFTW3 plan.
pub struct FFTPlan {
    #[cfg(feature = "single")]
    plan: NonNull<::fftw3_ffi::fftwf_plan_s>,
    #[cfg(not(feature = "single"))]
    plan: NonNull<::fftw3_ffi::fftw_plan_s>,
}

unsafe impl Send for FFTPlan {}
unsafe impl Sync for FFTPlan {}

pub enum FFTDirection {
    Forward = ::fftw3_ffi::FFTW_FORWARD as isize,
    Backward = ::fftw3_ffi::FFTW_BACKWARD as isize,
}

pub enum FFTFlags {
    Estimate = ::fftw3_ffi::FFTW_ESTIMATE as isize,
    Measure = ::fftw3_ffi::FFTW_MEASURE as isize,
    Patient = ::fftw3_ffi::FFTW_PATIENT as isize,
    /// This is equal to FFTW_MEASURE | FFTW_UNALIGNED
    Unaligned = ::fftw3_ffi::FFTW_UNALIGNED as isize,
    EstimateUnaligned = (::fftw3_ffi::FFTW_ESTIMATE | ::fftw3_ffi::FFTW_UNALIGNED) as isize,
}

impl FFTPlan {
    /// Create a new FFTW3 complex to complex plan.
    /// INFO: According to FFTW3 documentation r2c/c2r can be more efficient.
    /// WARNING: This is an unormalized transformation. A forwards and
    /// backwards transformation will lead to input data scaled by the number
    /// of elements.
    pub fn new_c2c_2d(
        ina: &mut ArrayViewMut<Complex<Float>, Ix2>,
        outa: &mut ArrayViewMut<Complex<Float>, Ix2>,
        direction: FFTDirection,
        flags: FFTFlags,
    ) -> Option<FFTPlan> {
        let (n0, n1) = ina.dim();
        let inp = ina.as_ptr() as *mut FFTWComplex;
        let outp = outa.as_ptr() as *mut FFTWComplex;

        let plan;
        // WARNING: Not thread safe!
        unsafe {
            plan = fftw_plan_dft_2d(
                n0 as std::os::raw::c_int,
                n1 as std::os::raw::c_int,
                inp,
                outp,
                direction as std::os::raw::c_int,
                flags as std::os::raw::c_uint,
            );
        }

        NonNull::new(plan).map(|p| FFTPlan { plan: p })
    }

    /// Create a new FFTW3 complex to complex plan for an inplace
    /// transformation.
    /// WARNING: This is an unormalized transformation. A forwards and
    /// backwards transformation will lead to input data scaled by the number
    /// of elements.
    /// TODO: Write test. Return Result, not Option.
    pub fn new_c2c_inplace_2d(
        arr: &mut ArrayViewMut<Complex<Float>, Ix2>,
        direction: FFTDirection,
        flags: FFTFlags,
    ) -> Option<FFTPlan> {
        let (n0, n1) = arr.dim();
        let p = arr.as_ptr() as *mut FFTWComplex;

        let plan;
        unsafe {
            plan = fftw_plan_dft_2d(
                n0 as std::os::raw::c_int,
                n1 as std::os::raw::c_int,
                p,
                p,
                direction as std::os::raw::c_int,
                flags as std::os::raw::c_uint,
            );
        }

        NonNull::new(plan).map(|p| FFTPlan { plan: p })
    }

    /// Create a new FFTW3 complex to complex plan.
    /// INFO: According to FFTW3 documentation r2c/c2r can be more efficient.
    /// WARNING: This is an unormalized transformation. A forwards and
    /// backwards transformation will lead to input data scaled by the number
    /// of elements.
    pub fn new_c2c_3d(
        ina: &mut ArrayViewMut<Complex<Float>, Ix3>,
        outa: &mut ArrayViewMut<Complex<Float>, Ix3>,
        direction: FFTDirection,
        flags: FFTFlags,
    ) -> Option<FFTPlan> {
        let (n0, n1, n2) = ina.dim();
        let inp = ina.as_ptr() as *mut FFTWComplex;
        let outp = outa.as_ptr() as *mut FFTWComplex;

        let plan;
        // WARNING: Not thread safe!
        unsafe {
            plan = fftw_plan_dft_3d(
                n0 as std::os::raw::c_int,
                n1 as std::os::raw::c_int,
                n2 as std::os::raw::c_int,
                inp,
                outp,
                direction as std::os::raw::c_int,
                flags as std::os::raw::c_uint,
            );
        }

        NonNull::new(plan).map(|p| FFTPlan { plan: p })
    }

    /// Create a new FFTW3 complex to complex plan for an inplace
    /// transformation.
    /// WARNING: This is an unormalized transformation. A forwards and
    /// backwards transformation will lead to input data scaled by the number
    /// of elements.
    /// TODO: Write test. Return Result, not Option.
    pub fn new_c2c_inplace_3d(
        arr: &mut ArrayViewMut<Complex<Float>, Ix3>,
        direction: FFTDirection,
        flags: FFTFlags,
    ) -> Option<FFTPlan> {
        let (n0, n1, n2) = arr.dim();
        let p = arr.as_ptr() as *mut FFTWComplex;

        let plan;
        unsafe {
            plan = fftw_plan_dft_3d(
                n0 as std::os::raw::c_int,
                n1 as std::os::raw::c_int,
                n2 as std::os::raw::c_int,
                p,
                p,
                direction as std::os::raw::c_int,
                flags as std::os::raw::c_uint,
            );
        }

        NonNull::new(plan).map(|p| FFTPlan { plan: p })
    }

    /// Create a new FFTW3 complex to complex plan for an inplace
    /// transformation.
    /// WARNING: This is an unormalized transformation. A forwards and
    /// backwards transformation will lead to input data scaled by the number
    /// of elements.
    /// TODO: Write test. Return Result, not Option.
    pub fn new_c2c_inplace_3d_dyn(
        arr: &mut ArrayViewMut<Complex<Float>, IxDyn>,
        direction: FFTDirection,
        flags: FFTFlags,
    ) -> Option<FFTPlan> {
        let dim = arr.dim();
        let p = arr.as_ptr() as *mut FFTWComplex;

        let plan;
        unsafe {
            plan = fftw_plan_dft_3d(
                dim[0] as std::os::raw::c_int,
                dim[1] as std::os::raw::c_int,
                dim[2] as std::os::raw::c_int,
                p,
                p,
                direction as std::os::raw::c_int,
                flags as std::os::raw::c_uint,
            );
        }

        NonNull::new(plan).map(|p| FFTPlan { plan: p })
    }

    /// Execute FFTW# plan for associated given input and output.
    pub fn execute(&self) {
        unsafe { fftw_execute(self.plan.as_ptr()) }
    }

    /// Reuse plan for different arrays
    /// TODO: Write test.
    pub fn reexecute2d(&self, a: &mut ArrayViewMut<Complex<Float>, Ix2>) {
        let p = a.as_ptr() as *mut FFTWComplex;
        unsafe {
            fftw_execute_dft(self.plan.as_ptr(), p, p);
        }
    }

    /// Reuse plan for different arrays
    /// TODO: Write test.
    pub fn reexecute3d(&self, a: &mut ArrayViewMut<Complex<Float>, Ix3>) {
        let p = a.as_ptr() as *mut FFTWComplex;
        unsafe {
            fftw_execute_dft(self.plan.as_ptr(), p, p);
        }
    }
}

/// Automatically destroy FFTW3 plan, when going out of scope.
impl Drop for FFTPlan {
    fn drop(&mut self) {
        unsafe {
            #[cfg(feature = "single")]
            ::fftw3_ffi::fftwf_destroy_plan(self.plan.as_mut());
            #[cfg(not(feature = "single"))]
            ::fftw3_ffi::fftw_destroy_plan(self.plan.as_mut());
        }
    }
}

pub fn fftw_init(nthreads: Option<usize>) -> Result<(), i32> {
    if cfg!(feature = "fftw-threaded") {
        if let Some(n) = nthreads {
            unsafe {
                #[cfg(feature = "single")]
                let code = ::fftw3_ffi::fftwf_init_threads();
                #[cfg(not(feature = "single"))]
                let code = ::fftw3_ffi::fftw_init_threads();
                if code == 0 {
                    return Err(code);
                };
                #[cfg(feature = "single")]
                ::fftw3_ffi::fftwf_plan_with_nthreads(n as i32);
                #[cfg(not(feature = "single"))]
                ::fftw3_ffi::fftw_plan_with_nthreads(n as i32);
            }
        }
    }

    Ok(())
}

pub fn fttw_finalize() {
    if cfg!(feature = "fftw-threaded") {
        unsafe {
            #[cfg(feature = "single")]
            ::fftw3_ffi::fftw_cleanup_threads();
            #[cfg(not(feature = "single"))]
            ::fftw3_ffi::fftwf_cleanup_threads();
        };
    }
}
