use brak_core::Result;
use brak_ir_lir::lir::LirProgram;

pub trait LirOptimizationPass: Send + Sync {
    fn name(&self) -> &'static str;
    fn run(&self, program: LirProgram) -> Result<LirProgram>;
}

pub struct PassManager {
    passes: Vec<Box<dyn LirOptimizationPass>>,
    _libraries: Vec<libloading::Library>,
    pub max_iterations: usize,
    pub verbose: bool,
}

unsafe impl Send for PassManager {}
unsafe impl Sync for PassManager {}

impl Default for PassManager {
    fn default() -> Self {
        Self { 
            passes: Vec::new(),
            _libraries: Vec::new(),
            max_iterations: 1,
            verbose: false,
        }
    }
}

impl PassManager {
    pub fn with_iterations(mut self, iterations: usize) -> Self {
        self.max_iterations = iterations;
        self
    }

    pub fn with_verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    pub fn add_pass(&mut self, pass: Box<dyn LirOptimizationPass>) {
        self.passes.push(pass);
    }

    /// Load an optimization pass from a dynamic library (.so, .dll, .dylib)
    /// The library must export a function: `pub extern "Rust" fn create_pass() -> Box<dyn LirOptimizationPass>`
    pub fn load_external_pass(&mut self, path: &str) -> Result<()> {
        unsafe {
            let lib = libloading::Library::new(path)
                .map_err(|e| format!("Failed to load plugin {}: {}", path, e))?;
            
            // We use extern "Rust" for simplicity in this toolkit.
            // This requires the plugin to be compiled with the same rustc version.
            let constructor: libloading::Symbol<fn() -> Box<dyn LirOptimizationPass>> = lib.get(b"create_pass")
                .map_err(|e| format!("Plugin {} does not export 'create_pass': {}", path, e))?;

            let pass = constructor();
            self.passes.push(pass);
            self._libraries.push(lib);
            Ok(())
        }
    }

    pub fn run(&self, mut program: LirProgram) -> Result<LirProgram> {
        use brak_core::ContentHash;

        if self.verbose {
            println!("[opt] Starting optimization pipeline with {} passes", self.passes.len());
        }

        for i in 0..self.max_iterations {
            let mut iteration_modified = false;

            for pass in &self.passes {
                let pass_start_hash = program.content_hash();
                program = pass.run(program)?;
                let pass_end_hash = program.content_hash();
                
                if pass_start_hash != pass_end_hash {
                    iteration_modified = true;
                    if self.verbose {
                        println!("[opt]   Pass '{}' modified the program", pass.name());
                    }
                }
            }

            if !iteration_modified {
                if self.verbose && self.max_iterations > 1 {
                    println!("[opt] Optimization converged after {} iterations", i + 1);
                }
                break;
            }

            if self.verbose && self.max_iterations > 1 {
                println!("[opt] Iteration {}/{} completed", i + 1, self.max_iterations);
            }
        }

        Ok(program)
    }
}
