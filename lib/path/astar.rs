use std::cmp::{Ord, Ordering, PartialEq, PartialOrd};
use std::collections::{BinaryHeap, HashMap};
use std::hash::{Hash, Hasher};

use super::*;

/// The Id which identifies a particular node and allows for comparisons
#[derive(Debug, Clone)]
struct Id<M>
where
    M: Model,
{
    /// Simple integer ID which must be unique
    id: usize,
    /// Estimated cost including the heuristic
    f: M::Cost,
    /// Cost to arrive at this node following the parents
    g: M::Cost,
}

impl<M> Hash for Id<M>
where
    M: Model,
{
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.id.hash(state);
    }
}

impl<M> PartialEq for Id<M>
where
    M: Model,
{
    fn eq(&self, other: &Self) -> bool {
        self.f == other.f
    }
}

impl<M> Eq for Id<M> where M: Model {}

impl<M> PartialOrd for Id<M>
where
    M: Model,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(other.f.cmp(&self.f))
    }
}

impl<M> Ord for Id<M>
where
    M: Model,
{
    fn cmp(&self, other: &Self) -> Ordering {
        other.f.cmp(&self.f)
    }
}

/// Nodes stored for planning
#[derive(Clone, Debug)]
struct Node<M>
where
    M: Model,
{
    id: Id<M>,
    state: M::State,
    control: M::Control,
}

impl<M> PartialEq for Node<M>
where
    M: Model,
{
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl<M> Eq for Node<M> where M: Model {}

impl<M> PartialOrd for Node<M>
where
    M: Model,
{
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        self.id.partial_cmp(&other.id)
    }
}

impl<M> Ord for Node<M>
where
    M: Model,
{
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

#[derive(Debug)]
pub struct AStar<M>
where
    M: HeuristicModel,
{
    queue: BinaryHeap<Node<M>>,
    parent_map: HashMap<Id<M>, Node<M>>,
    grid: HashMap<<<M as Model>::State as State>::Position, Id<M>>,
    id_counter: usize,
}

impl<M> AStar<M>
where
    M: HeuristicModel,
{
    /// Create a new AStar optimizer
    pub fn new() -> Self {
        AStar {
            queue: BinaryHeap::new(),
            parent_map: HashMap::new(),
            grid: HashMap::new(),
            id_counter: 0,
        }
    }

    pub fn inspect_queue(&self) -> impl Iterator<Item = (&M::State, &M::Control)> {
        self.queue.iter().map(|node| (&node.state, &node.control))
    }

    pub fn inpect_discovered(
        &self,
    ) -> impl Iterator<Item = &<<M as Model>::State as State>::Position> {
        self.grid.keys()
    }

    #[inline(always)]
    fn step<S>(
        &mut self,
        current: &Node<M>,
        model: &mut M,
        goal: &M::State,
        sampler: &mut S,
    ) -> bool
    where
        S: Sampler<M>,
    {
        if model.converge(&current.state, goal) {
            return true;
        }

        for control in sampler.sample(model, &current.state) {
            if !model.is_valid(&current.state, &control) {
                continue;
            }

            self.id_counter += 1;
            let child_state = model.integrate(&current.state, &control);

            let cost = current.id.g.clone() + model.cost(&current.state, &child_state);
            let heuristic = model.heuristic(&child_state, goal);

            let child = Node::<M> {
                id: Id { id: self.id_counter, g: cost.clone(), f: cost + heuristic },
                state: child_state,
                control: control.clone(),
            };

            let best =
                self.grid.entry(child.state.grid_position()).or_insert(child.id.clone());

            if *best < child.id {
                continue;
            } else {
                self.parent_map.insert(child.id.clone(), current.clone());
                self.queue.push(child);
            }
        }

        false
    }

    /// Follow the parents from the goal node up to the start node
    fn unwind_trajectory(&self, model: &M, mut current: Node<M>) -> Trajectory<M> {
        let mut result = Vec::new();
        result.push((current.state.clone(), current.control.clone()));
        let mut cost = M::Cost::default();

        // build up the trajectory by following the parent nodes
        loop {
            if let Some(p) = self.parent_map.get(&current.id) {
                cost = cost + model.cost(&current.state, &p.state);
                current = (*p).clone();
                result.push((current.state.clone(), current.control.clone()));
            } else {
                break;
            }
        }

        result.reverse();

        Trajectory { cost, trajectory: result }
    }
}

impl<M, S> Optimizer<M, S> for AStar<M>
where
    M: HeuristicModel,
    S: Sampler<M>,
{
    fn next_trajectory(
        &mut self,
        model: &mut M,
        start: &M::State,
        goal: &M::State,
        sampler: &mut S,
    ) -> Result<M> {
        use PathFindingErr::*;

        if self.parent_map.is_empty() && self.queue.is_empty() {
            let start_id =
                Id { id: 0, g: Default::default(), f: model.heuristic(start, goal) };
            self.queue.push(Node {
                id: start_id,
                state: start.clone(),
                control: Default::default(),
            });
        }

        if let Some(current) = self.queue.pop() {
            self.step(&current, model, &goal, sampler);
            Ok(self.unwind_trajectory(model, current))
        } else {
            Err(Unreachable)
        }
    }

    fn optimize(
        &mut self,
        model: &mut M,
        start: M::State,
        goal: M::State,
        sampler: &mut S,
    ) -> Result<M> {
        use PathFindingErr::*;

        if model.converge(&start, &goal) {
            return Ok(Trajectory {
                cost: Default::default(),
                trajectory: vec![(start, Default::default())],
            });
        }

        let start_id = Id { id: 0, g: Default::default(), f: model.heuristic(&start, &goal) };
        self.queue.push(Node { id: start_id, state: start, control: Default::default() });

        while let Some(current) = self.queue.pop() {
            if self.step(&current, model, &goal, sampler) {
                return Ok(self.unwind_trajectory(model, current));
            }
        }

        Err(Unreachable)
    }
}
