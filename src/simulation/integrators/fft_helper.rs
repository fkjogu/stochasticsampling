use consts::TWOPI;
use fftw3::complex::Complex;
use ndarray::{Array, Ix1, Ix3};
use simulation::settings::{BoxSize, GridSize};


/// Returns a sampling of k values along all grid axes in FFTW standard form.
/// In this case 2D.
///
/// For a grid size of `n`, the 0th-mode is at index `0`. For even n the index
/// `n/2` represents both the largest positive and negative frequency. For odd
/// `n` index `(n-1)/2` is the largest positive frequency and `(n+1)/2` the
/// largest negative frequency. For the values at index `i`, `-i = n-k` holds
/// true.
///
/// Example:
///     n = 10 => k = [0, 1, 2, 3, 4, (5, -5), -4, -3, -2, -1]
///     n = 11 => k = [0, 1, 2, 3, 4, 5, -5, -4, -3, -2, -1]
///
fn get_k_sampling(grid_size: GridSize, box_size: BoxSize) -> Vec<Array<Complex<f64>, Ix1>> {
    let ks: Vec<Array<Complex<f64>, Ix1>> = grid_size[..2]
        .iter()
        .zip(box_size[..2].iter())
        .map(|(gs, bs)| {
            let a = (gs / 2) as isize;
            let b = if gs % 2 == 0 { gs / 2 } else { gs / 2 + 1 } as isize;
            let step = TWOPI / bs;

            let values: Array<Complex<f64>, Ix1> =
                Array::from_vec((-(a as i64)..(b as i64))
                                    .into_iter()
                                    .map(|i| Complex::new((i as f64) * step, 0.))
                                    .collect());

            let mut k = Array::from_elem(*gs, Complex::new(0., 0.));

            k.slice_mut(s![..b]).assign(&values.slice(s![a..]));
            k.slice_mut(s![b..]).assign(&values.slice(s![..a]));

            k
        })
        .collect();

    ks
}


/// Returns a meshgrid of k values for FFT.
///
/// Every grid point contains values of a 2D k vector
///     `res[i,j] -> [k1[i], k2[j]]`
pub fn get_k_mesh(grid_size: GridSize, box_size: BoxSize) -> Array<Complex<f64>, Ix3> {
    let ks = get_k_sampling(grid_size, box_size);

    let mut res = Array::from_elem([grid_size[0], grid_size[1], 2], Complex::new(0., 0.));

    for (kx, mut ax0) in ks[0].iter().zip(res.outer_iter_mut()) {
        for (ky, mut val) in ks[1].iter().zip(ax0.outer_iter_mut()) {
            val[0] = *kx;
            val[1] = *ky;
        }
    }

    res
}



#[cfg(test)]
mod tests {
    use super::*;
    use ndarray::arr3;
    use test_helper::equal_floats;


    #[test]
    fn test_get_k_sampling() {
        let bs = [6., 7.];
        let gs = [6, 7, 1];

        let k = get_k_sampling(gs, bs);

        let expect0 = [0.,
                       1.0471975511965976,
                       2.0943951023931953,
                       -3.1415926535897931,
                       -2.0943951023931953,
                       -1.0471975511965976];
        let expect1 = [0.,
                       0.8975979010256552,
                       1.7951958020513104,
                       2.6927937030769655,
                       -2.6927937030769655,
                       -1.7951958020513104,
                       -0.8975979010256552];

        for (v, e) in k[0].iter().zip(&expect0) {
            assert!(equal_floats(v.re(), *e), "{} != {}", v.re(), *e);
        }

        for (v, e) in k[1].iter().zip(&expect1) {
            assert!(equal_floats(v.re(), *e), "{} != {}", v.re(), *e);
        }
    }


    #[test]
    fn test_get_k_mesh() {
        let bs = [TWOPI, TWOPI];
        let gs = [4, 3, 1];

        let mesh = get_k_mesh(gs, bs);

        let expect = arr3(&[[[0., 0.], [0., 1.], [0., -1.]],
                            [[1., 0.], [1., 1.], [1., -1.]],
                            [[-2., 0.], [-2., 1.], [-2., -1.]],
                            [[-1., 0.], [-1., 1.], [-1., -1.]]]);

        for (v, e) in mesh.iter().zip(expect.iter()) {
            assert!(equal_floats(v.re(), *e), "{} != {}", v.re(), *e);
        }
    }

}
