use ndarray::{parallel::prelude::*, s, Array1, Array2, ArrayView1};
use ndarray_linalg::{error::LinalgError, EigValsh, InverseC, UPLO};

use crate::{
    f_,
    gp::GP,
    kernel::{Kernel, KernelState},
    utils::{Array2Utils, Array3Utils, Array4Utils, ArrayBaseUtils, ArrayView2Utils},
};

pub trait HyperparameterOptimizer {
    fn log_lik(&self) -> f_;

    fn log_lik_with_prior(&self, base_thetas: ArrayView1<f_>) -> f_;

    fn log_lik_jac(&self) -> Array1<f_>;

    fn log_lik_jac_with_prior(&self, base_thetas: ArrayView1<f_>) -> Array1<f_>;

    fn log_lik_hess(&self) -> Array2<f_>;

    fn optimize_thetas(&mut self) -> Result<(), LinalgError>;

    fn backtrack(&mut self, base_log_lik: f64, base_thetas: Array1<f_>, backtrack_base: f_, backtrack_n: i32, delta: Array1<f_>) -> Result<(), LinalgError>;
}

impl<kern: Kernel> HyperparameterOptimizer for GP<kern> {
    fn log_lik(&self) -> f_ {
        match self.kernel.state() {
            KernelState::Fitted => assert!(true),
            KernelState::Unfitted => {
                panic!("Cannot calc log_lik with unfitted GP Model!") // switch to anyhow error?
            }
        }

        -0.5 * (self.mem.y_m().dot(&self.alpha))[0] - self.L.factor.diag().map(|val| val.ln()).sum() //Precalc L trace?
        //Prior over ln length scales
        // -0.5 * self.prior_sigma.powi(2).recip() * self.kernel.ln_l().dot(&self.kernel.ln_l()) // UNCOMMENT IN WORKING VER
        //Prior over sigma_f

    }

    fn log_lik_with_prior(&self, base_thetas: ArrayView1<f_>) -> f_ {
        // -0.5 * (self.mem.y_m().dot(&self.alpha))[0] - self.L.factor.diag().map(|val| val.ln()).sum() //Precalc L trace?
        self.log_lik()
        //Prior over ln length scales
        -0.5 * self.prior_sigma.powi(2).recip() * self.kernel.ln_l().dot(&self.kernel.ln_l())
        //Prior over sigma_f
        -0.5 * self.prior_sigma.powi(2).recip() * (self.kernel.sigma_f().ln() - base_thetas[0]).powi(2)

    }

    //checked
    fn log_lik_jac(&self) -> Array1<f_> {
        match self.kernel.state() {
            KernelState::Fitted => assert!(true),
            KernelState::Unfitted => {
                panic!("Cannot calc log_lik grad with unfitted GP Model!") // switch to anyhow error?
            }
        }

        let inner = &self.alpha.dot(&self.alpha.t()) - &self.Kinv;

        let grad: Array1<f_> = self
            .kernel
            .calc_thetas_jac(&self.K, &self.mem)
            .outer_iter()
            .map(|jac| 0.5 * jac.product_trace(&inner.view()))
            .collect();

        grad

    }

    fn log_lik_jac_with_prior(&self, base_thetas: ArrayView1<f_>) -> Array1<f_> {
        let mut grad = self.log_lik_jac();

        grad.indexed_iter_mut()
            .take(1)
            .for_each(|(_, val)| *val -= self.kernel.sigma_f().ln() / self.prior_sigma.powi(2));

        grad.indexed_iter_mut()
            .take(1)
            .for_each(|(_, val)| *val -= base_thetas[0] / self.prior_sigma.powi(2));

        grad.indexed_iter_mut()
            .skip(1)
            .for_each(|(i, val)| *val -= self.kernel.ln_l()[i - 1] / self.prior_sigma.powi(2));

        grad
    }

    // Calcs -H so that invc can be used, checked
    fn log_lik_hess(&self) -> Array2<f_> {
        match self.kernel.state() {
            KernelState::Fitted => assert!(true),
            KernelState::Unfitted => {
                panic!("Cannot calc log_lik hess with unfitted GP Model!") // switch to anyhow error?
            }
        }

        let mut intermed_comp = self.kernel.calc_thetas_jac(&self.K, &self.mem);
        intermed_comp
            .outer_iter_mut()
            .for_each(|mut jac| jac.assign(&self.Kinv.dot(&jac)));

        let hess = self.kernel.calc_thetas_hess(&self.K, &self.mem);

        let hess_fill_fn = |(i, j)| -> f_ {
            0.5 * hess.slice(s![i, j, .., ..]).product_trace(&self.Kinv.view()) // tr(AB) = tr(BA)
                - 0.5 * intermed_comp.outer(i).product_trace(&intermed_comp.outer(j))
                + self
                    .mem
                    .y_m()
                    .dot(&intermed_comp.outer(i))
                    .dot(&intermed_comp.outer(j))
                    .dot(&self.alpha)[0]
                - 0.5 * self.alpha.t().dot(&hess.outer(i, j)).dot(&self.alpha)[(0, 0)]
        };

        let mut hess = Array2::zeros((self.dim + 1, self.dim + 1))
            .map_UPLO(UPLO::Upper, hess_fill_fn)
            .fill_with_UPLO(UPLO::Upper);

        // let mut hess = Array2::from_shape_fn((self.dim + 1, self.dim + 1), hess_fill_fn);

        let prior_sigma = self.prior_sigma;
        
        // hess.slice_mut(s![1.., 1..])
        //     .diag_mut()
        //     .par_mapv_inplace(|val| val + 1.0 / prior_sigma.powi(2));

        hess.diag_mut()
            .mapv_inplace(|val| val + 1.0 / prior_sigma.powi(2));

        hess
        // hess.fill_with_UPLO(UPLO::Upper)
    }


    // WORKING
    // fn optimize_thetas(&mut self) -> Result<(), LinalgError> {
    //     let mut thetas = self.kernel.thetas().to_owned().ln();

    //     // let new_sigma_f =
    //     //     ((1.0 / self.mem.n() as f64) * (self.mem.y_m().dot(&self.alpha))[0]).sqrt();
    //     // //Moore et. al.

    //     let mut new_sigma_f = (self.mem.y_prime_std_dev()).ln();
    //     if new_sigma_f.is_infinite() {
    //         //if std_dev is zero
    //         new_sigma_f = (0.01 as f_).ln();
    //     }

    //     thetas[0] = new_sigma_f;
    //     self.kernel.update_thetas(&thetas.clone().exp());
    //     self.fit()?;

    //     let base_log_lik = self.log_lik();
    //     let base_thetas = thetas;

    //     let mut hess = self.log_lik_hess();
    //     hess = hess.slice(s![1.., 1..,]).to_owned();

    //     let mut grad = self.log_lik_jac();
    //     grad = grad.slice(s![1..,]).to_owned();

    //     let eigs = hess.eigvalsh(UPLO::Lower);

    //     if eigs.is_ok()
    //         && eigs.iter().all(|eig_res| {
    //             eig_res
    //                 .par_iter()
    //                 .all(|eig| eig.is_normal() && eig.is_sign_positive())
    //         })
    //     {

    //         if let Ok(hess_inv) = hess.invc() {
    //             let delta = hess_inv.dot(&grad);
            
    //             self.backtrack(base_log_lik, base_thetas.clone(), 0.5, 5, delta)?;
    //             if self.log_lik() > base_log_lik {
    //                 // println!("Hess steps: {}", i + 1);
    //                 return Ok(());
    //             }
    //         }
            
    //     }

    //     self.backtrack(base_log_lik, base_thetas.clone(), 0.1, 5, grad)?;

    //     if self.log_lik() > base_log_lik {
    //         // println!("Grad steps: {}", i + 1);
    //         return Ok(());
    //     } else {
    //         self.kernel.update_thetas(&base_thetas.exp());
    //         self.fit()?;
    //         return Ok(());
    //     }
    // }

    // fn backtrack(&mut self, base_log_lik: f64, base_thetas: Array1<f_>, backtrack_base: f_, backtrack_n: i32, delta: Array1<f_>) -> Result<(), LinalgError> {
    //     for i in 0..backtrack_n {
    //         let mut thetas = base_thetas.clone();
    //         thetas
    //             .slice_mut(s![2..])
    //             .iter_mut()
    //             .zip(delta.iter())
    //             .for_each(|(theta, delta)| *theta = backtrack_base.powi(i) as f_ * delta); //check sign

    //         self.kernel.update_thetas(&thetas.to_owned().exp());
    //         self.fit()?;
    //         // println!("Hess");
    //         if self.log_lik() > base_log_lik {
    //             // println!("Hess steps: {}", i + 1);
    //             return Ok(());
    //         }
    //     }

    //     Ok(())
    // }

    fn optimize_thetas(&mut self) -> Result<(), LinalgError> {
    
        
        let mut thetas = self.kernel.thetas().to_owned().ln();
        println!("thetas {}", thetas);

        println!("std dev {}", self.mem.y_prime_std_dev().ln());
        // let new_sigma_f =
        //     ((1.0 / self.mem.n() as f64) * (self.mem.y_m().dot(&self.alpha))[0]).sqrt();
        // //Moore et. al.

        let mut new_sigma_f = (self.mem.y_prime_std_dev()).ln();
        if new_sigma_f.is_infinite() {
            //if std_dev is zero
            new_sigma_f = (0.01 as f_).ln();
        }

        thetas[0] = new_sigma_f;
        self.kernel.update_thetas(&thetas.clone().exp());
        self.fit()?;

        let base_log_lik = self.log_lik_with_prior(thetas.view());
        let base_thetas = thetas;

        let mut hess = self.log_lik_hess();
        // hess = hess.slice(s![1.., 1..,]).to_owned();

        println!("hess {}", hess);

        let mut grad = self.log_lik_jac_with_prior(base_thetas.view());
        // grad = grad.slice(s![1..,]).to_owned();

        println!("grad {}", grad);

        let eigs = hess.eigvalsh(UPLO::Lower);

        println!("eigs {:?}", eigs);

        if eigs.is_ok()
            && eigs.iter().all(|eig_res| {
                eig_res
                    .par_iter()
                    .all(|eig| eig.is_normal() && eig.is_sign_positive())
            })
        {

            if let Ok(hess_inv) = hess.invc() {
                let delta = hess_inv.dot(&grad);
                println!("delta {}", delta);
                self.backtrack(base_log_lik, base_thetas.clone(), 0.5, 10, delta)?;
                todo!();
                if self.log_lik_with_prior(base_thetas.view()) > base_log_lik {
                    // println!("Hess steps: {}", i + 1);
                    return Ok(());
                }
            }
            
        }

        self.backtrack(base_log_lik, base_thetas.clone(), 0.1, 5, grad)?;

        if self.log_lik_with_prior(base_thetas.view()) > base_log_lik {
            // println!("Grad steps: {}", i + 1);
            return Ok(());
        } else {
            self.kernel.update_thetas(&base_thetas.exp());
            self.fit()?;
            return Ok(());
        }
    }

    fn backtrack(&mut self, base_log_lik: f64, base_thetas: Array1<f_>, backtrack_base: f_, backtrack_n: i32, delta: Array1<f_>) -> Result<(), LinalgError> {
        for i in 0..backtrack_n {
            let mut thetas = base_thetas.clone();

            thetas[0] = backtrack_base.powi(i) as f_ * delta[0];
            
            thetas
                .slice_mut(s![2..])
                .iter_mut()
                .zip(delta.iter().skip(1))
                .for_each(|(theta, delta)| *theta = backtrack_base.powi(i) as f_ * delta); //check sign

            self.kernel.update_thetas(&thetas.to_owned().exp());
            self.fit()?;
            // println!("Hess");
            println!("test lik {}", self.log_lik_with_prior(base_thetas.view()));
            if self.log_lik_with_prior(base_thetas.view()) > base_log_lik {
                // println!("Hess steps: {}", i + 1);
                println!("suc new thetas {}", thetas);
                return Ok(());
            }
        }
        Ok(())
    }


}

// OLD BEFORE BACKTRACK REFACTOR
// fn optimize_thetas(&mut self) -> Result<(), LinalgError> {
//     let mut thetas = self.kernel.thetas().to_owned().ln();

//     // let new_sigma_f =
//     //     ((1.0 / self.mem.n() as f64) * (self.mem.y_m().dot(&self.alpha))[0]).sqrt();
//     // //Moore et. al.

//     let mut new_sigma_f = (self.mem.y_prime_std_dev()).ln();
//     if new_sigma_f.is_infinite() {
//         //if std_dev is zero
//         new_sigma_f = (0.01 as f_).ln();
//     }

//     thetas[0] = new_sigma_f;
//     self.kernel.update_thetas(&thetas.clone().exp());
//     self.fit()?;

//     let base_log_lik = self.log_lik();
//     let base_thetas = thetas;

//     let mut hess = self.log_lik_hess();
//     hess = hess.slice(s![1.., 1..,]).to_owned();

//     let mut grad = self.log_lik_jac();
//     grad = grad.slice(s![1..,]).to_owned();

//     let eigs = hess.eigvalsh(UPLO::Lower);

//     if eigs.is_ok()
//         && eigs.iter().all(|eig_res| {
//             eig_res
//                 .par_iter()
//                 .all(|eig| eig.is_normal() && eig.is_sign_positive())
//         })
//     {
//         for i in 0..5 {
//             // Second order step with backtracking
//             let hess_inv = match hess.invc() {
//                 Ok(inv) => inv,
//                 Err(_) => break, //fall back to grad ascent
//             };
//             let delta = hess_inv.dot(&grad);
//             let mut thetas = base_thetas.clone();
//             thetas
//                 .slice_mut(s![2..])
//                 .iter_mut()
//                 .zip(delta.iter())
//                 .for_each(|(theta, delta)| *theta = 0.5_f64.powi(i) as f_ * delta); //check sign

//             self.kernel.update_thetas(&thetas.to_owned().exp());
//             self.fit()?;
//             // println!("Hess");
//             if self.log_lik() > base_log_lik {
//                 // println!("Hess steps: {}", i + 1);
//                 return Ok(());
//             }
//         }
//     }

//     for i in 0..5 {
//         // Gradient ascent with backtracking
//         let mut thetas = base_thetas.clone();
//         thetas
//             .slice_mut(s![2..])
//             .iter_mut()
//             .zip(grad.iter())
//             .for_each(|(theta, grad)| *theta += 0.1_f64.powi(i) as f_ * grad);

//         self.kernel.update_thetas(&thetas.to_owned().exp());
//         match self.fit() {
//             Ok(_) => assert!(true),
//             Err(_) => continue,
//         };
//         if self.log_lik() > base_log_lik {
//             // println!("Grad steps: {}", i + 1);
//             return Ok(());
//         }
//     }

//     // dbg!(&thetas);

//     self.kernel.update_thetas(&base_thetas.exp());
//     self.fit()?;

//     // dbg!(self.log_lik());

//     Ok(())
// }