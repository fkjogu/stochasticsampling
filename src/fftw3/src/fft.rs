use ndarray::{ArrayViewMut, Ix};
use complex::Complex64;

pub type FFTWComplex = ::fftw3_ffi::fftw_complex;

/// Wrapper to manage the state of an FFTW3 plan.
pub struct FFTPlan {
    plan: ::fftw3_ffi::fftw_plan,
}

#[derive(Copy, Clone)]
pub enum FFTDirection {
    Forward = ::fftw3_ffi::FFTW_FORWARD as isize,
    Backward = ::fftw3_ffi::FFTW_BACKWARD as isize,
}

impl FFTPlan {
    /// Create a new FFTW3 complex to complex plan.
    /// INFO: According to FFTW3 documentation r2c/c2r can be more efficient.
    /// WARNING: This is an unormalized transformation. A forwards and
    /// backwards transformation will lead to input data scaled by the number
    /// of elements.
    pub fn new_c2c(ina: &mut ArrayViewMut<Complex64, (Ix, Ix)>,
                   outa: &mut ArrayViewMut<Complex64, (Ix, Ix)>,
                   direction: FFTDirection)
                   -> FFTPlan {

        let (n0, n1) = ina.dim();
        let inp = &mut ina[[0, 0]] as *mut _ as *mut FFTWComplex;
        let outp = &mut outa[[0, 0]] as *mut _ as *mut FFTWComplex;

        let plan;

        unsafe {
            plan = ::fftw3_ffi::fftw_plan_dft_2d(n0 as i32,
                                                 n1 as i32,
                                                 inp,
                                                 outp,
                                                 direction as i32,
                                                 ::fftw3_ffi::FFTW_ESTIMATE as u32);
            // TODO: Use measure here
        }

        FFTPlan { plan: plan }
    }

    /// Execute FFTW# plan for associated given input and output.
    pub fn execute(self) {
        unsafe { ::fftw3_ffi::fftw_execute(self.plan) }
    }
}

/// Automatically destroy FFTW3 plan, when going out of scope.
impl Drop for FFTPlan {
    fn drop(&mut self) {
        unsafe {
            ::fftw3_ffi::fftw_destroy_plan(self.plan);
        }
    }
}
