//! The composition root (ADR-0003).

use tetsu_kernel::Module;

fn main() {
    println!("{}", summary(&[]));
}

fn summary(modules: &[Box<dyn Module>]) -> String {
    let names: Vec<&str> = modules.iter().map(|module| module.name()).collect();
    let listed = if names.is_empty() {
        "none".to_string()
    } else {
        names.join(", ")
    };
    format!("tetsu {}, modules: {listed}", env!("CARGO_PKG_VERSION"))
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Named(&'static str);

    impl Module for Named {
        fn name(&self) -> &'static str {
            self.0
        }
    }

    #[test]
    fn summary_says_when_no_modules_are_registered() {
        assert_eq!(
            summary(&[]),
            format!("tetsu {}, modules: none", env!("CARGO_PKG_VERSION"))
        );
    }

    #[test]
    fn summary_names_each_registered_module() {
        let modules: Vec<Box<dyn Module>> =
            vec![Box::new(Named("tenancy")), Box::new(Named("booking"))];
        assert_eq!(
            summary(&modules),
            format!(
                "tetsu {}, modules: tenancy, booking",
                env!("CARGO_PKG_VERSION")
            )
        );
    }
}
