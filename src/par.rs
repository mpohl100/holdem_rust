//! Safe Rust translation of the C++ `par` scheduler and calculation helpers.
//!
//! The source declarations are in `cpp_code/src/par/Scheduler.h` and
//! `cpp_code/src/par/Calculation.h`. Rust names use `snake_case`; each item
//! documents its corresponding C++ name. The original scheduler read task
//! queues and `cancel_` outside its mutex, which is a data race. This
//! translation checks and updates both under one mutex. Dropping a scheduler
//! also requests cancellation and joins its workers; otherwise the C++ worker
//! loops could prevent destruction from completing. `Scheduler` is not
//! `Clone`, corresponding to its deleted C++ copy operations. Rust ownership
//! still permits moving it, replacing the source's deleted move operations
//! with safe ownership transfer.

use std::collections::{BTreeMap, VecDeque};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::Duration;

/// A reusable, thread-safe task; maps `Scheduler::Task`.
pub type Task = Arc<dyn Fn() + Send + Sync + 'static>;

/// Failure returned when tasks are submitted to a scheduler with no workers.
///
/// The corresponding C++ round-robin expression would divide by zero in this
/// case, so Rust reports the invalid configuration instead of reproducing
/// undefined behavior.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SchedulerError;

/// Thread pool that distributes submitted tasks over worker-specific queues;
/// maps C++ `par::Scheduler`.
pub struct Scheduler {
    /// Worker thread handles; maps `threads_`.
    workers: Vec<JoinHandle<()>>,
    /// Shared task queues and cancellation flag; maps `tasks_`, `mutex_`, and
    /// `cancel_` as one mutex-protected state.
    state: Arc<Mutex<SchedulerState>>,
}

/// Mutable worker-shared state; this replaces the independently accessed C++
/// fields protected by `Scheduler::mutex_`.
struct SchedulerState {
    /// One FIFO task queue per worker; maps `tasks_`.
    tasks: Vec<VecDeque<Task>>,
    /// Whether workers should exit after their assigned queues are drained;
    /// maps `cancel_`.
    cancelled: bool,
}

impl Scheduler {
    /// Creates `num_threads` workers; maps `Scheduler(size_t numThreads)`.
    ///
    /// Each worker starts immediately and polls its queue every 250 ms when
    /// idle, preserving the source's polling interval.
    #[must_use]
    pub fn new(num_threads: usize) -> Self {
        let state = Arc::new(Mutex::new(SchedulerState {
            tasks: (0..num_threads).map(|_| VecDeque::new()).collect(),
            cancelled: false,
        }));
        let mut scheduler = Self {
            workers: Vec::with_capacity(num_threads),
            state,
        };

        for index in 0..num_threads {
            let worker_state = Arc::clone(&scheduler.state);
            scheduler
                .workers
                .push(thread::spawn(move || run_worker(&worker_state, index)));
        }

        scheduler
    }

    /// Runs one worker loop on the calling thread; maps `run(size_t i)`.
    ///
    /// `index` must name an existing worker queue.
    ///
    /// # Panics
    ///
    /// Panics if `index` does not name a worker queue or the shared state
    /// mutex has been poisoned.
    pub fn run(&self, index: usize) {
        run_worker(&self.state, index);
    }

    /// Requests cancellation; maps `cancel()`.
    ///
    /// Workers finish tasks already queued for them and then stop. The flag is
    /// synchronized with queue access to correct the source data race.
    ///
    /// # Panics
    ///
    /// Panics if the scheduler state mutex has been poisoned.
    pub fn cancel(&self) {
        self.state
            .lock()
            .expect("scheduler state mutex poisoned")
            .cancelled = true;
    }

    /// Appends tasks in round-robin order starting at worker zero; maps
    /// `appendTasks(const std::vector<Task>&)`.
    ///
    /// The C++ implementation restarts its distribution index at zero for
    /// each call, which this method preserves. Nonempty input with zero
    /// workers returns [`SchedulerError`] instead of dividing by zero.
    ///
    /// # Errors
    ///
    /// Returns [`SchedulerError`] when tasks are submitted with no workers.
    ///
    /// # Panics
    ///
    /// Panics if the scheduler state mutex has been poisoned.
    pub fn append_tasks(&self, tasks: Vec<Task>) -> Result<(), SchedulerError> {
        let mut state = self.state.lock().expect("scheduler state mutex poisoned");
        if !tasks.is_empty() && state.tasks.is_empty() {
            return Err(SchedulerError);
        }
        distribute_tasks(&mut state.tasks, tasks);
        Ok(())
    }
}

impl Default for Scheduler {
    /// Creates an empty scheduler without workers; maps `Scheduler() = default`.
    fn default() -> Self {
        Self {
            workers: Vec::new(),
            state: Arc::new(Mutex::new(SchedulerState {
                tasks: Vec::new(),
                cancelled: false,
            })),
        }
    }
}

impl Drop for Scheduler {
    /// Requests cancellation and joins workers before shared state is dropped.
    fn drop(&mut self) {
        self.cancel();
        for worker in self.workers.drain(..) {
            worker.join().expect("scheduler worker panicked");
        }
    }
}

/// Distributes tasks to queues, restarting the round-robin index on each call.
fn distribute_tasks(queues: &mut [VecDeque<Task>], tasks: Vec<Task>) {
    if queues.is_empty() {
        return;
    }
    let queue_count = queues.len();
    for (index, task) in tasks.into_iter().enumerate() {
        queues[index % queue_count].push_back(task);
    }
}

/// Runs a worker until cancellation is requested and its own queue is empty.
fn run_worker(state: &Arc<Mutex<SchedulerState>>, index: usize) {
    loop {
        let task = {
            let mut shared = state.lock().expect("scheduler state mutex poisoned");
            let queue = shared
                .tasks
                .get_mut(index)
                .expect("worker index does not name a scheduler queue");
            match queue.pop_front() {
                Some(task) => Some(task),
                None if shared.cancelled => return,
                None => None,
            }
        };

        if let Some(task) = task {
            task();
        } else {
            thread::sleep(Duration::from_millis(250));
        }
    }
}

/// One stored callable and argument tuple; maps C++ `SubCalculation<Result, Args...>`.
///
/// `Output` maps the C++ `Result`; `Args` is the Rust tuple corresponding to
/// the source's variadic template argument list. The Rust callable receives
/// that tuple as one argument, avoiding an artificial arity limit.
#[derive(Clone)]
pub struct SubCalculation<Output, Args> {
    /// Optional callable; `None` represents the empty C++ default-constructed
    /// `std::function`.
    func: Option<Arc<dyn Fn(Args) -> Output + Send + Sync>>,
    /// Stored argument tuple; maps `args_`.
    args: Args,
    /// Output marker; maps the `Result` template parameter.
    output: std::marker::PhantomData<fn() -> Output>,
}

impl<Output, Args> SubCalculation<Output, Args>
where
    Args: Clone,
{
    /// Creates a subcalculation from a callable and its argument tuple; maps
    /// `SubCalculation(f, args...)`.
    pub fn new<F>(func: F, args: Args) -> Self
    where
        F: Fn(Args) -> Output + Send + Sync + 'static,
    {
        Self {
            func: Some(Arc::new(func)),
            args,
            output: std::marker::PhantomData,
        }
    }

    /// Calculates one result using the stored tuple; maps `calc() const`.
    ///
    /// Calling a default-constructed subcalculation panics, corresponding to
    /// invoking an empty C++ `std::function`.
    ///
    /// # Panics
    ///
    /// Panics if this subcalculation has no stored callable.
    pub fn calc(&self) -> Output {
        self.func
            .as_ref()
            .expect("cannot calculate with an empty SubCalculation")(self.args.clone())
    }

    /// Returns the stored argument tuple; maps `args()`.
    pub fn args(&self) -> &Args {
        &self.args
    }

    /// Returns mutable access to the stored argument tuple; maps the mutable
    /// overload of `args()`.
    pub fn args_mut(&mut self) -> &mut Args {
        &mut self.args
    }
}

impl<Output, Args: Default> Default for SubCalculation<Output, Args> {
    /// Creates an empty callable and default tuple; maps `SubCalculation()`.
    fn default() -> Self {
        Self {
            func: None,
            args: Args::default(),
            output: std::marker::PhantomData,
        }
    }
}

/// Calculation object behavior needed by [`CalcStep`].
///
/// `Result` maps the C++ nested alias `SubCalculation::result_t`.
pub trait SubCalculationLike: Clone {
    /// Result type produced by `calc()`; maps `result_t`.
    type Result: Clone + Default;

    /// Executes the stored subcalculation; maps `SubCalculation::calc()`.
    fn calc(&self) -> Self::Result;
}

impl<Output, Args> SubCalculationLike for SubCalculation<Output, Args>
where
    Output: Clone + Default,
    Args: Clone,
{
    type Result = Output;

    /// Executes the callable over a clone of its stored tuple.
    fn calc(&self) -> Self::Result {
        SubCalculation::calc(self)
    }
}

/// A set of independent subcalculations with indexed results; maps
/// C++ `CalcStep<SubCalc>`.
pub struct CalcStep<SubCalc: SubCalculationLike> {
    /// Shared lock; maps `mutex_`. Cloned steps share this mutex as in the
    /// source, while retaining their copied data.
    mutex: Arc<Mutex<()>>,
    /// Subcalculations executed by this step; maps `subCalcs_`.
    sub_calculations: Vec<SubCalc>,
    /// Progress state protected by `mutex`; maps `currentIndex_` and
    /// `results_`.
    progress: Mutex<CalcStepProgress<SubCalc::Result>>,
}

/// Mutex-protected progress data belonging to a calculation step.
struct CalcStepProgress<Output> {
    /// Index returned by the next `pop`; maps `currentIndex_`.
    current_index: usize,
    /// Results keyed by subcalculation index; maps `results_`.
    results: BTreeMap<usize, Output>,
}

impl<SubCalc: SubCalculationLike> CalcStep<SubCalc> {
    /// Creates a calculation step from subcalculations; maps
    /// `CalcStep(const std::vector<SubCalc>&)`.
    #[must_use]
    pub fn new(sub_calculations: Vec<SubCalc>) -> Self {
        Self {
            mutex: Arc::new(Mutex::new(())),
            sub_calculations,
            progress: Mutex::new(CalcStepProgress {
                current_index: 0,
                results: BTreeMap::new(),
            }),
        }
    }

    /// Returns the next subcalculation and its index, or `None` when exhausted;
    /// maps `pop()` and its `{nullopt, size_t(-1)}` sentinel.
    ///
    /// # Panics
    ///
    /// Panics if either calculation-step mutex has been poisoned.
    pub fn pop(&self) -> Option<(SubCalc, usize)> {
        let _guard = self.mutex.lock().expect("calculation step mutex poisoned");
        let mut progress = self
            .progress
            .lock()
            .expect("calculation progress mutex poisoned");
        let index = progress.current_index;
        let sub_calculation = self.sub_calculations.get(index)?.clone();
        progress.current_index += 1;
        Some((sub_calculation, index))
    }

    /// Stores the result at an index; maps `setResult(index, result)`.
    ///
    /// # Panics
    ///
    /// Panics if either calculation-step mutex has been poisoned.
    pub fn set_result(&self, index: usize, result: &SubCalc::Result) {
        let _guard = self.mutex.lock().expect("calculation step mutex poisoned");
        self.progress
            .lock()
            .expect("calculation progress mutex poisoned")
            .results
            .insert(index, result.clone());
    }

    /// Calculates every not-yet-popped subcalculation on the calling thread;
    /// maps `calc()`.
    pub fn calc(&self) {
        while let Some((sub_calculation, index)) = self.pop() {
            let result = sub_calculation.calc();
            self.set_result(index, &result);
        }
    }

    /// Returns mutable access to the subcalculations; maps mutable
    /// `subCalcs()`.
    pub fn sub_calculations_mut(&mut self) -> &mut Vec<SubCalc> {
        &mut self.sub_calculations
    }

    /// Returns shared access to the subcalculations; maps const `subCalcs()`.
    pub fn sub_calculations(&self) -> &[SubCalc] {
        &self.sub_calculations
    }

    /// Returns the result at `index`, or the result type's default value when
    /// no result is present; maps `result(index)`.
    ///
    /// # Panics
    ///
    /// Panics if either calculation-step mutex has been poisoned.
    pub fn result(&self, index: usize) -> SubCalc::Result {
        let _guard = self.mutex.lock().expect("calculation step mutex poisoned");
        self.progress
            .lock()
            .expect("calculation progress mutex poisoned")
            .results
            .get(&index)
            .cloned()
            .unwrap_or_default()
    }
}

impl<SubCalc: SubCalculationLike> Default for CalcStep<SubCalc> {
    /// Creates an empty step; maps `CalcStep()`.
    fn default() -> Self {
        Self::new(Vec::new())
    }
}

impl<SubCalc> Clone for CalcStep<SubCalc>
where
    SubCalc: SubCalculationLike,
    SubCalc::Result: Clone,
{
    /// Copies the step data while retaining the shared mutex, as the C++
    /// default copy constructor does for its `shared_ptr<std::mutex>`.
    fn clone(&self) -> Self {
        let _guard = self.mutex.lock().expect("calculation step mutex poisoned");
        let progress = self
            .progress
            .lock()
            .expect("calculation progress mutex poisoned");
        Self {
            mutex: Arc::clone(&self.mutex),
            sub_calculations: self.sub_calculations.clone(),
            progress: Mutex::new(CalcStepProgress {
                current_index: progress.current_index,
                results: progress.results.clone(),
            }),
        }
    }
}

/// Transformation callable executed between consecutive calculation steps;
/// maps an element of C++ `transformations_`.
pub type Transformation = Box<dyn FnMut() + Send + 'static>;

/// Provides the two initialization hooks corresponding to the C++ virtual
/// methods `initCalculations()` and `initTransformations()`.
///
/// C++ subclass overrides become implementations of this trait. The hook
/// receives the destination vector so it can add calculation steps and
/// transformations without exposing the `Calculation` fields.
pub trait CalculationStages<SubCalc: SubCalculationLike> {
    /// Populates the calculation-step vector; maps `initCalculations()`.
    fn init_calculations(&mut self, calculations: &mut Vec<CalcStep<SubCalc>>);

    /// Populates the transformation vector; maps `initTransformations()`.
    fn init_transformations(&mut self, transformations: &mut Vec<Transformation>);
}

/// Multi-stage calculation runner; maps C++ `Calculation<SubCalc>`.
///
/// The C++ class's virtual initialization hooks are implemented by
/// [`CalculationStages`]. Transformations are stored as `FnMut` callables and
/// therefore are not cloned; unlike `std::function`, Rust closures have no
/// general-purpose deep-clone operation.
pub struct Calculation<SubCalc: SubCalculationLike> {
    /// Worker count used for every calculation step; maps `numThreads_`.
    num_threads: usize,
    /// Ordered calculation stages; maps `calculations_`.
    calculations: Vec<CalcStep<SubCalc>>,
    /// Transformations from one stage to the next; maps `transformations_`.
    transformations: Vec<Transformation>,
}

impl<SubCalc: SubCalculationLike> Calculation<SubCalc> {
    /// Creates a runner with `num_threads` workers per step; maps
    /// `Calculation(size_t numThreads)`.
    #[must_use]
    pub fn new(num_threads: usize) -> Self {
        Self {
            num_threads,
            calculations: Vec::new(),
            transformations: Vec::new(),
        }
    }

    /// Calls both initialization hooks in source order; maps `init()`.
    ///
    /// Rust has no virtual methods, so the C++ subclass is passed as a
    /// [`CalculationStages`] implementation.
    pub fn init(&mut self, stages: &mut impl CalculationStages<SubCalc>) {
        stages.init_calculations(&mut self.calculations);
        stages.init_transformations(&mut self.transformations);
    }

    /// Calculates stages in order, joins each stage's workers, then runs the
    /// corresponding transformation before proceeding; maps `calculate()`.
    ///
    /// The C++ `std::thread` vector is replaced by scoped safe threads. A zero
    /// worker count preserves the source's behavior of starting no calculation
    /// workers while still running required inter-stage transformations.
    ///
    /// # Panics
    ///
    /// Panics if a calculation panics or an inter-stage transformation is
    /// missing from the configured stages.
    pub fn calculate(&mut self)
    where
        SubCalc: Sync,
        SubCalc::Result: Send,
    {
        for index in 0..self.calculations.len() {
            let calculation = &self.calculations[index];
            thread::scope(|scope| {
                for _ in 0..self.num_threads {
                    scope.spawn(move || calculation.calc());
                }
            });

            if index < self.calculations.len() - 1 {
                let transformation = self
                    .transformations
                    .get_mut(index)
                    .expect("missing transformation between calculation stages");
                transformation();
            }
        }
    }

    /// Adds a calculation step; maps insertion into protected `calculations_`.
    pub fn add_calculation(&mut self, calculation: CalcStep<SubCalc>) {
        self.calculations.push(calculation);
    }

    /// Adds a transformation; maps insertion into protected `transformations_`.
    pub fn add_transformation<F>(&mut self, transformation: F)
    where
        F: FnMut() + Send + 'static,
    {
        self.transformations.push(Box::new(transformation));
    }

    /// Returns the number of workers per stage; maps `numThreads_`.
    #[must_use]
    pub fn num_threads(&self) -> usize {
        self.num_threads
    }

    /// Returns the ordered calculation stages and their results.
    #[must_use]
    pub fn calculations(&self) -> &[CalcStep<SubCalc>] {
        &self.calculations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    /// Verifies that each task is assigned exactly once in round-robin order.
    #[test]
    fn scheduler_distributes_tasks_round_robin() {
        let mut queues = (0..3).map(|_| VecDeque::new()).collect::<Vec<_>>();
        let tasks = (0..7).map(|_| Arc::new(|| {}) as Task).collect::<Vec<_>>();
        distribute_tasks(&mut queues, tasks);

        assert_eq!(
            queues.iter().map(VecDeque::len).collect::<Vec<_>>(),
            vec![3, 2, 2]
        );
    }

    /// Verifies zero-worker submissions report the source's undefined case
    /// instead of attempting a modulo-by-zero operation.
    #[test]
    fn scheduler_rejects_tasks_without_workers() {
        let scheduler = Scheduler::default();
        let task: Task = Arc::new(|| {});
        assert_eq!(scheduler.append_tasks(vec![task]), Err(SchedulerError));
    }

    /// Verifies cancellation drains queued tasks and dropping waits for
    /// completion.
    #[test]
    fn scheduler_cancel_drains_tasks_and_joins_workers() {
        let completed = Arc::new(AtomicUsize::new(0));
        let scheduler = Scheduler::new(3);
        let tasks = (0..12)
            .map(|_| {
                let completed = Arc::clone(&completed);
                Arc::new(move || {
                    completed.fetch_add(1, Ordering::SeqCst);
                }) as Task
            })
            .collect();
        scheduler
            .append_tasks(tasks)
            .expect("workers are available");
        scheduler.cancel();
        drop(scheduler);

        assert_eq!(completed.load(Ordering::SeqCst), 12);
    }

    type IntegerCalculation = SubCalculation<i32, (i32,)>;

    /// Supplies calculation stages to exercise C++ virtual-hook equivalents.
    struct TestStages {
        /// Shared event log for verifying initialization and execution order.
        events: Arc<Mutex<Vec<&'static str>>>,
    }

    impl CalculationStages<IntegerCalculation> for TestStages {
        /// Adds one calculation stage containing two independent tasks.
        fn init_calculations(&mut self, calculations: &mut Vec<CalcStep<IntegerCalculation>>) {
            self.events
                .lock()
                .expect("test event mutex poisoned")
                .push("init_calculations");
            let increment = |events: Arc<Mutex<Vec<&'static str>>>| {
                move |(value,)| {
                    events
                        .lock()
                        .expect("test event mutex poisoned")
                        .push("first_task");
                    value + 1
                }
            };
            calculations.push(CalcStep::new(vec![
                SubCalculation::new(increment(Arc::clone(&self.events)), (4,)),
                SubCalculation::new(increment(Arc::clone(&self.events)), (8,)),
            ]));

            let second_events = Arc::clone(&self.events);
            let second_stage = move |(value,)| {
                second_events
                    .lock()
                    .expect("test event mutex poisoned")
                    .push("second_task");
                value * 2
            };
            calculations.push(CalcStep::new(vec![SubCalculation::new(second_stage, (3,))]));
        }

        /// Adds the transformation between the two calculation stages.
        fn init_transformations(&mut self, transformations: &mut Vec<Transformation>) {
            self.events
                .lock()
                .expect("test event mutex poisoned")
                .push("init_transformations");
            let events = Arc::clone(&self.events);
            transformations.push(Box::new(move || {
                events
                    .lock()
                    .expect("test event mutex poisoned")
                    .push("transform");
            }));
        }
    }

    /// Verifies initialization order, stage/transformation order, and results.
    #[test]
    fn calculation_runs_stages_in_order_and_stores_results() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let mut stages = TestStages {
            events: Arc::clone(&events),
        };
        let mut calculation = Calculation::new(1);
        calculation.init(&mut stages);
        calculation.calculate();

        assert_eq!(
            *events.lock().expect("test event mutex poisoned"),
            vec![
                "init_calculations",
                "init_transformations",
                "first_task",
                "first_task",
                "transform",
                "second_task"
            ]
        );
        assert_eq!(calculation.calculations()[0].result(0), 5);
        assert_eq!(calculation.calculations()[0].result(1), 9);
        assert_eq!(calculation.calculations()[1].result(0), 6);
    }

    /// Verifies tuple arguments are not limited to a fixed function arity.
    #[test]
    fn subcalculation_accepts_nine_arguments_in_its_tuple() {
        let calculation = SubCalculation::new(
            |values: (i32, i32, i32, i32, i32, i32, i32, i32, i32)| {
                let (first, second, third, fourth, fifth, sixth, seventh, eighth, ninth) = values;
                first + second + third + fourth + fifth + sixth + seventh + eighth + ninth
            },
            (1, 2, 3, 4, 5, 6, 7, 8, 9),
        );

        assert_eq!(calculation.calc(), 45);
    }
}
