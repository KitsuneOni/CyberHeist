//! Pure Rust model for choosing a route through a contract.
//!
//! The graph and route-locking rules live here so they can be tested without
//! Godot. "Map" is only the domain name for this node graph: this module does
//! not define a visual map, UI layout, or final UX. Presentation and scene
//! changes are handled separately by the Godot adapter.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::fmt;

use rand::Rng;
use rand::seq::SliceRandom;

pub type NodeId = u32;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EncounterType {
    Entry,
    Combat,
    Event,
    Shop,
    Elite,
    /// The security system guarding the end of a zone. One per zone, always
    /// alone in its column, and the only way through to the zone after it.
    Boss,
}

impl EncounterType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Entry => "entry",
            Self::Combat => "combat",
            Self::Event => "event",
            Self::Shop => "shop",
            Self::Elite => "elite",
            Self::Boss => "boss",
        }
    }

    pub fn is_boss(self) -> bool {
        matches!(self, Self::Boss)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EncounterNode {
    id: NodeId,
    encounter_type: EncounterType,
    next_nodes: Vec<NodeId>,
}

impl EncounterNode {
    pub fn new(
        id: NodeId,
        encounter_type: EncounterType,
        next_nodes: impl Into<Vec<NodeId>>,
    ) -> Self {
        Self {
            id,
            encounter_type,
            next_nodes: next_nodes.into(),
        }
    }

    pub fn id(&self) -> NodeId {
        self.id
    }

    pub fn encounter_type(&self) -> EncounterType {
        self.encounter_type
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EncounterSelection {
    pub node_id: NodeId,
    pub encounter_type: EncounterType,
    /// Which zone the chosen encounter is in, so the construct guarding it
    /// can be scaled to the depth the player has reached.
    pub zone: usize,
}

/// Where a node stands from the player's point of view, for drawing the map.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NodeStatus {
    /// Finished, and the route moved on from it.
    Completed,
    /// Where the player is standing right now.
    Current,
    /// Reachable from the current node and not locked out.
    Available,
    /// Ruled out by choosing a different branch.
    Locked,
    /// In a later zone, behind a boss that has not been beaten yet. Distinct
    /// from `Locked`, which is permanent: a sealed node opens up again the
    /// moment the zone's boss goes down.
    Sealed,
    /// Further along the contract, not reachable yet.
    Upcoming,
}

impl NodeStatus {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Current => "current",
            Self::Available => "available",
            Self::Locked => "locked",
            Self::Sealed => "sealed",
            Self::Upcoming => "upcoming",
        }
    }
}

/// A node plus its status, so the map screen can draw the whole contract.
#[derive(Clone, Debug, PartialEq)]
pub struct NodeProgress {
    pub node_id: NodeId,
    pub encounter_type: EncounterType,
    pub status: NodeStatus,
    /// Horizontal position, 0.0 at the entry column and 1.0 at the target.
    pub x: f32,
    /// Vertical position within the column, 0.0 top to 1.0 bottom.
    pub y: f32,
    /// The node a run starts from.
    pub is_entry: bool,
    /// Where the player is standing. Tracked apart from `status`, because a
    /// node the player has cleared is both completed and where they are, and
    /// the map needs to show both at once.
    pub is_current: bool,
    /// A node nothing leads on from, i.e. the contract objective. The two
    /// fixed points of a run are drawn differently from the encounters the
    /// player chooses between.
    pub is_target: bool,
    /// The security system at the end of its zone. Drawn apart from the
    /// ordinary encounters, since it is the gate rather than a choice.
    pub is_boss: bool,
    /// Which zone this node belongs to, counting from 0.
    pub zone: usize,
    /// Nodes this one leads to, for drawing the lines between them.
    pub connections: Vec<NodeId>,
}

/// How far through the contract's zones the player is, for the map screen.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ZoneProgress {
    /// Zone the player is standing in, counting from 0.
    pub current_zone: usize,
    /// Highest zone opened up so far. Equal to `current_zone` until a boss
    /// goes down and the next zone's nodes become enterable.
    pub unlocked_zones: usize,
    /// Zones on this contract. A contract with no boss nodes is one zone.
    pub total_zones: usize,
}

/// How far through the contract the player is.
///
/// The entry node is excluded: it is where a run starts, not an encounter the
/// player completes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContractProgress {
    pub completed: usize,
    pub total: usize,
}

/// How a generated contract is shaped.
///
/// A contract is a run of zones. Each zone is `zone_length` columns of
/// ordinary encounters followed by a single boss column, so the boss is
/// always the last node of its zone and the only way into the next one.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ContractShape {
    /// Zones on the contract, each ending in its own boss.
    pub zone_count: usize,
    /// Columns of ordinary encounters in a zone, before its boss.
    pub zone_length: usize,
    /// Fewest and most parallel routes a middle column can offer.
    pub min_width: usize,
    pub max_width: usize,
}

impl Default for ContractShape {
    fn default() -> Self {
        Self {
            zone_count: 3,
            zone_length: 2,
            min_width: 2,
            max_width: 3,
        }
    }
}

impl ContractShape {
    /// Columns from the entry to the final boss, counting both ends: the
    /// entry, then each zone's encounters and the boss closing it.
    pub fn length(&self) -> usize {
        1 + self.zone_count.max(1) * (self.zone_length + 1)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ContractMapError {
    EmptyMap,
    DuplicateNode(NodeId),
    DuplicateConnection { from: NodeId, to: NodeId },
    UnknownStartNode(NodeId),
    UnknownConnection { from: NodeId, to: NodeId },
    CyclicGraph,
    EncounterAlreadyInProgress(NodeId),
    NoEncounterInProgress,
    EncounterNotReachable { from: NodeId, requested: NodeId },
    ZoneSealed { requested: NodeId, zone: usize },
}

impl fmt::Display for ContractMapError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::EmptyMap => formatter.write_str("the contract map contains no encounters"),
            Self::DuplicateNode(node_id) => {
                write!(formatter, "encounter {node_id} appears more than once")
            }
            Self::DuplicateConnection { from, to } => {
                write!(
                    formatter,
                    "encounter {from} links to encounter {to} more than once"
                )
            }
            Self::UnknownStartNode(node_id) => {
                write!(formatter, "starting encounter {node_id} does not exist")
            }
            Self::UnknownConnection { from, to } => {
                write!(
                    formatter,
                    "encounter {from} leads to missing encounter {to}"
                )
            }
            Self::CyclicGraph => formatter.write_str("the contract map contains a cycle"),
            Self::EncounterAlreadyInProgress(node_id) => write!(
                formatter,
                "encounter {node_id} is already in progress; another route cannot be chosen"
            ),
            Self::NoEncounterInProgress => formatter.write_str("there is no active map encounter"),
            Self::EncounterNotReachable { from, requested } => write!(
                formatter,
                "encounter {requested} is unavailable from encounter {from}"
            ),
            Self::ZoneSealed { requested, zone } => write!(
                formatter,
                "encounter {requested} is in zone {} and stays sealed until this zone's boss is beaten",
                zone + 1
            ),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ContractMap {
    nodes: BTreeMap<NodeId, EncounterNode>,
    /// Where the run began. `current_node` moves as the player advances, so
    /// the entry point is kept separately for drawing the map.
    start_node: NodeId,
    current_node: NodeId,
    locked_nodes: BTreeSet<NodeId>,
    active_node: Option<NodeId>,
    completed_nodes: BTreeSet<NodeId>,
    /// Highest zone the player may enter. Raised only by beating a boss, so
    /// the nodes past one stay sealed however reachable the graph makes them.
    unlocked_zones: usize,
}

impl ContractMap {
    pub fn new(
        nodes: impl IntoIterator<Item = EncounterNode>,
        start_node: NodeId,
    ) -> Result<Self, ContractMapError> {
        let mut node_map = BTreeMap::new();

        for node in nodes {
            let node_id = node.id;
            if node_map.insert(node_id, node).is_some() {
                return Err(ContractMapError::DuplicateNode(node_id));
            }
        }

        if node_map.is_empty() {
            return Err(ContractMapError::EmptyMap);
        }
        if !node_map.contains_key(&start_node) {
            return Err(ContractMapError::UnknownStartNode(start_node));
        }
        for node in node_map.values() {
            let mut connections = BTreeSet::new();
            for next_node in &node.next_nodes {
                if !connections.insert(*next_node) {
                    return Err(ContractMapError::DuplicateConnection {
                        from: node.id,
                        to: *next_node,
                    });
                }
                if !node_map.contains_key(next_node) {
                    return Err(ContractMapError::UnknownConnection {
                        from: node.id,
                        to: *next_node,
                    });
                }
            }
        }
        if graph_contains_cycle(&node_map) {
            return Err(ContractMapError::CyclicGraph);
        }

        Ok(Self {
            nodes: node_map,
            start_node,
            current_node: start_node,
            locked_nodes: BTreeSet::new(),
            active_node: None,
            completed_nodes: BTreeSet::new(),
            unlocked_zones: 0,
        })
    }

    /// Builds a fresh contract, so no two runs follow the same route.
    ///
    /// Nodes are laid out in columns between a single entry and a single
    /// target. Edges only ever run to the next column, which keeps the graph
    /// acyclic by construction, and every node is given at least one way in
    /// and one way out, so whichever branch the player commits to still
    /// reaches the target.
    pub fn generate(shape: ContractShape, rng: &mut impl Rng) -> Self {
        let length = shape.length().max(2);
        let min_width = shape.min_width.max(1);
        let max_width = shape.max_width.max(min_width);
        // Every `zone_length` columns of encounters are closed off by a boss
        // column, so column 3, 6, 9... end zone 1, 2, 3... of a default
        // contract. The last of them is also the contract's target.
        let zone_stride = shape.zone_length + 1;
        let is_boss_column =
            |column_index: usize| column_index > 0 && column_index.is_multiple_of(zone_stride);

        // Column 0 is the entry and every boss column holds a single node:
        // the fixed points of a run are not chosen between.
        let mut columns: Vec<Vec<NodeId>> = Vec::with_capacity(length);
        let mut next_id: NodeId = 0;

        for column_index in 0..length {
            let width = if column_index == 0 || is_boss_column(column_index) {
                1
            } else {
                rng.random_range(min_width..=max_width)
            };

            let mut column = Vec::with_capacity(width);
            for _ in 0..width {
                column.push(next_id);
                next_id += 1;
            }
            columns.push(column);
        }

        // Every node gets one or two ways forward, then any node in the next
        // column nothing reached is wired up, so no route is left stranded.
        let mut connections: BTreeMap<NodeId, BTreeSet<NodeId>> = BTreeMap::new();
        for pair in columns.windows(2) {
            let (current, next) = (&pair[0], &pair[1]);

            for node_id in current {
                let mut candidates = next.clone();
                candidates.shuffle(rng);
                let take = rng.random_range(1..=2.min(candidates.len()));
                connections
                    .entry(*node_id)
                    .or_default()
                    .extend(candidates.into_iter().take(take));
            }

            for node_id in next {
                let reached = current
                    .iter()
                    .any(|from| connections.get(from).is_some_and(|to| to.contains(node_id)));
                if !reached {
                    let from = current[rng.random_range(0..current.len())];
                    connections.entry(from).or_default().insert(*node_id);
                }
            }
        }

        let nodes: Vec<EncounterNode> = columns
            .iter()
            .enumerate()
            .flat_map(|(column_index, column)| {
                column.iter().map(move |node_id| {
                    let encounter_type = if column_index == 0 {
                        EncounterType::Entry
                    } else if is_boss_column(column_index) {
                        EncounterType::Boss
                    } else {
                        EncounterType::Combat
                    };
                    (*node_id, encounter_type)
                })
            })
            .map(|(node_id, encounter_type)| {
                let next_nodes: Vec<NodeId> = connections
                    .get(&node_id)
                    .map(|set| set.iter().copied().collect())
                    .unwrap_or_default();
                EncounterNode::new(node_id, encounter_type, next_nodes)
            })
            .collect();

        let mut contract = Self::new(nodes, 0)
            .expect("a generated contract is acyclic and fully connected by construction");
        contract.assign_encounter_types(rng);
        contract
    }

    /// Gives the encounters between the entry and the target their types.
    ///
    /// Done as a second pass so the mix is chosen over the whole contract
    /// rather than column by column.
    fn assign_encounter_types(&mut self, rng: &mut impl Rng) {
        let start = self.start_node;
        for node in self.nodes.values_mut() {
            // The entry, the bosses and the target are placed by the shape of
            // the contract, so the mix is only rolled for the rest.
            if node.id == start || node.next_nodes.is_empty() || node.encounter_type.is_boss() {
                continue;
            }

            // Mostly fights, with quieter nodes mixed through so a route has
            // some texture to it.
            node.encounter_type = match rng.random_range(0..100u8) {
                0..=54 => EncounterType::Combat,
                55..=74 => EncounterType::Event,
                75..=89 => EncounterType::Shop,
                _ => EncounterType::Elite,
            };
        }
    }

    /// Authored data used by the tests. The two opening routes rejoin before
    /// the final elite encounter.
    pub fn demo() -> Self {
        Self::new(
            [
                EncounterNode::new(0, EncounterType::Entry, vec![1, 2]),
                EncounterNode::new(1, EncounterType::Combat, vec![3]),
                EncounterNode::new(2, EncounterType::Event, vec![4]),
                EncounterNode::new(3, EncounterType::Shop, vec![5]),
                EncounterNode::new(4, EncounterType::Combat, vec![5]),
                EncounterNode::new(5, EncounterType::Elite, vec![]),
            ],
            0,
        )
        .expect("the built-in contract graph must be valid")
    }

    /// Authored data used by the zone tests: two zones, each opening on a
    /// branch and closing on its own boss. Node 3 is zone 1's boss and node 6
    /// is zone 2's, which is also the contract target.
    #[cfg(test)]
    pub fn demo_zoned() -> Self {
        Self::new(
            [
                EncounterNode::new(0, EncounterType::Entry, vec![1, 2]),
                EncounterNode::new(1, EncounterType::Combat, vec![3]),
                EncounterNode::new(2, EncounterType::Event, vec![3]),
                EncounterNode::new(3, EncounterType::Boss, vec![4, 5]),
                EncounterNode::new(4, EncounterType::Combat, vec![6]),
                EncounterNode::new(5, EncounterType::Shop, vec![6]),
                EncounterNode::new(6, EncounterType::Boss, vec![]),
            ],
            0,
        )
        .expect("the built-in zoned contract graph must be valid")
    }

    pub fn current_encounter(&self) -> &EncounterNode {
        self.nodes
            .get(&self.current_node)
            .expect("a validated contract contains its current node")
    }

    pub fn selectable_encounters(&self) -> Vec<&EncounterNode> {
        if self.active_node.is_some() {
            return Vec::new();
        }

        let zones = self.node_zones();
        self.current_encounter()
            .next_nodes
            .iter()
            .filter(|node_id| !self.locked_nodes.contains(node_id))
            // A zone the player has not unlocked yet is not on offer, however
            // directly the graph leads into it.
            .filter(|node_id| zones.get(node_id).copied().unwrap_or(0) <= self.unlocked_zones)
            .map(|node_id| {
                self.nodes
                    .get(node_id)
                    .expect("a validated contract only links to existing nodes")
            })
            .collect()
    }

    pub fn encounter_option(
        &self,
        requested_node: NodeId,
    ) -> Result<EncounterSelection, ContractMapError> {
        if let Some(active_node) = self.active_node {
            return Err(ContractMapError::EncounterAlreadyInProgress(active_node));
        }

        if self.locked_nodes.contains(&requested_node)
            || !self
                .current_encounter()
                .next_nodes
                .contains(&requested_node)
        {
            return Err(ContractMapError::EncounterNotReachable {
                from: self.current_node,
                requested: requested_node,
            });
        }

        let zone = self.node_zones().get(&requested_node).copied().unwrap_or(0);
        if zone > self.unlocked_zones {
            return Err(ContractMapError::ZoneSealed {
                requested: requested_node,
                zone,
            });
        }

        let encounter = self
            .nodes
            .get(&requested_node)
            .expect("a validated contract only links to existing nodes");
        Ok(EncounterSelection {
            node_id: encounter.id,
            encounter_type: encounter.encounter_type,
            zone,
        })
    }

    pub fn select_encounter(
        &mut self,
        requested_node: NodeId,
    ) -> Result<EncounterSelection, ContractMapError> {
        let selection = self.encounter_option(requested_node)?;
        let unchosen_branches: Vec<_> = self
            .current_encounter()
            .next_nodes
            .iter()
            .copied()
            .filter(|node_id| *node_id != requested_node)
            .collect();

        self.locked_nodes.extend(unchosen_branches);
        self.active_node = Some(requested_node);
        Ok(selection)
    }

    pub fn complete_current_encounter(&mut self) -> Result<(), ContractMapError> {
        let completed_node = self
            .active_node
            .take()
            .ok_or(ContractMapError::NoEncounterInProgress)?;

        // The node being left is behind the player now, which matters for the
        // entry node: it is never an encounter that gets completed, but it
        // should not keep reading as somewhere still ahead.
        let departed_node = self.current_node;
        self.current_node = completed_node;
        self.completed_nodes.insert(completed_node);
        self.completed_nodes.insert(departed_node);

        // Beating the boss is the only thing that opens the next zone, so the
        // nodes behind it stop being sealed from here on.
        let cleared_a_boss = self
            .nodes
            .get(&completed_node)
            .is_some_and(|node| node.encounter_type.is_boss());
        if cleared_a_boss {
            let zone = self.node_zones().get(&completed_node).copied().unwrap_or(0);
            self.unlocked_zones = self.unlocked_zones.max(zone + 1);
        }
        Ok(())
    }

    /// Which zone each node sits in, counting from 0.
    ///
    /// A zone runs up to and including its boss, so the boss closes the zone
    /// it belongs to and the column after it opens the next. Derived from
    /// where the bosses sit rather than stored on the nodes, so an authored
    /// contract and a generated one agree without either declaring its zones.
    fn node_zones(&self) -> BTreeMap<NodeId, usize> {
        let depths = self.node_depths();
        let boss_depths: BTreeSet<usize> = self
            .nodes
            .values()
            .filter(|node| node.encounter_type.is_boss())
            .filter_map(|node| depths.get(&node.id).copied())
            .collect();

        depths
            .iter()
            .map(|(node_id, depth)| {
                let zone = boss_depths.iter().filter(|boss| *boss < depth).count();
                (*node_id, zone)
            })
            .collect()
    }

    /// Which zone the player is in, how many have been opened up, and how
    /// many the contract has in all.
    pub fn zone_progress(&self) -> ZoneProgress {
        let zones = self.node_zones();
        ZoneProgress {
            current_zone: zones.get(&self.current_node).copied().unwrap_or(0),
            unlocked_zones: self.unlocked_zones,
            // A contract with no boss on it is a single zone.
            total_zones: zones.values().copied().max().unwrap_or(0) + 1,
        }
    }

    /// How far each node sits from the entry, measured along the longest path
    /// that reaches it.
    ///
    /// Longest rather than shortest, so a node every branch reconverges on
    /// lands in the final column instead of being pulled left by whichever
    /// route happened to be shorter.
    fn node_depths(&self) -> BTreeMap<NodeId, usize> {
        let mut depths: BTreeMap<NodeId, usize> = BTreeMap::new();
        depths.insert(self.start_node, 0);

        // The graph is validated as acyclic, so repeatedly relaxing every edge
        // settles after at most one pass per node.
        for _ in 0..self.nodes.len() {
            let mut changed = false;
            for node in self.nodes.values() {
                let Some(depth) = depths.get(&node.id).copied() else {
                    continue;
                };
                for next_node in &node.next_nodes {
                    let candidate = depth + 1;
                    if depths
                        .get(next_node)
                        .is_none_or(|current| candidate > *current)
                    {
                        depths.insert(*next_node, candidate);
                        changed = true;
                    }
                }
            }
            if !changed {
                break;
            }
        }

        // A node no route reaches still needs somewhere to sit.
        for node_id in self.nodes.keys() {
            depths.entry(*node_id).or_insert(0);
        }

        depths
    }

    /// Nodes still reachable by walking forward from where the player is
    /// standing, without passing through a branch that has been locked out.
    ///
    /// Anything outside this set is unreachable for the rest of the contract,
    /// however far ahead it sits, which is what the map needs in order to
    /// stop describing dead routes as "further ahead".
    fn reachable_from_current(&self) -> BTreeSet<NodeId> {
        let mut reachable = BTreeSet::new();
        let mut queue = VecDeque::new();
        queue.push_back(self.current_node);

        while let Some(node_id) = queue.pop_front() {
            if self.locked_nodes.contains(&node_id) || !reachable.insert(node_id) {
                continue;
            }

            if let Some(node) = self.nodes.get(&node_id) {
                for next_node in &node.next_nodes {
                    if !self.locked_nodes.contains(next_node) {
                        queue.push_back(*next_node);
                    }
                }
            }
        }

        reachable
    }

    /// Every node with its current status and position, in id order, for
    /// drawing the map.
    pub fn node_progress(&self) -> Vec<NodeProgress> {
        let selectable: BTreeSet<NodeId> = self
            .selectable_encounters()
            .iter()
            .map(|node| node.id)
            .collect();
        let reachable = self.reachable_from_current();
        let zones = self.node_zones();
        let depths = self.node_depths();
        let max_depth = depths.values().copied().max().unwrap_or(0);

        // Nodes sharing a column are spread evenly down it, so a branch reads
        // as parallel routes rather than a single line.
        let mut columns: BTreeMap<usize, Vec<NodeId>> = BTreeMap::new();
        for (node_id, depth) in &depths {
            columns.entry(*depth).or_default().push(*node_id);
        }

        self.nodes
            .values()
            .map(|node| {
                let zone = zones.get(&node.id).copied().unwrap_or(0);
                let status = if self.completed_nodes.contains(&node.id) {
                    NodeStatus::Completed
                } else if node.id == self.current_node {
                    NodeStatus::Current
                } else if !reachable.contains(&node.id) {
                    // Covers the branch passed over and everything that could
                    // only have been reached through it. Checked before the
                    // zone: being locked out is permanent, and that is the
                    // more useful thing to tell the player.
                    NodeStatus::Locked
                } else if zone > self.unlocked_zones {
                    NodeStatus::Sealed
                } else if selectable.contains(&node.id) {
                    NodeStatus::Available
                } else {
                    NodeStatus::Upcoming
                };

                let depth = depths.get(&node.id).copied().unwrap_or(0);
                let x = if max_depth == 0 {
                    0.5
                } else {
                    depth as f32 / max_depth as f32
                };

                let column = &columns[&depth];
                let row = column.iter().position(|id| *id == node.id).unwrap_or(0);
                let y = (row + 1) as f32 / (column.len() + 1) as f32;

                NodeProgress {
                    node_id: node.id,
                    encounter_type: node.encounter_type,
                    status,
                    x,
                    y,
                    is_entry: node.id == self.start_node,
                    is_current: node.id == self.current_node,
                    is_target: node.next_nodes.is_empty(),
                    is_boss: node.encounter_type.is_boss(),
                    zone,
                    connections: node.next_nodes.clone(),
                }
            })
            .collect()
    }

    /// Completed encounters out of the number on a route through the
    /// contract.
    ///
    /// Counted along the route rather than over every node in the graph:
    /// committing to a branch locks its siblings out for good, so a total
    /// counting those too could never be reached and would read as a failure
    /// on a finished contract.
    pub fn progress(&self) -> ContractProgress {
        // One encounter per column after the entry.
        let total = self.node_depths().values().copied().max().unwrap_or(0);

        let completed = self
            .completed_nodes
            .iter()
            .filter(|node_id| {
                self.nodes
                    .get(node_id)
                    .is_some_and(|node| node.encounter_type != EncounterType::Entry)
            })
            .count();

        ContractProgress { completed, total }
    }

    /// Ends a failed encounter without advancing past it. The chosen route
    /// remains committed, so the same encounter is the only retry offered.
    pub fn fail_current_encounter(&mut self) -> Result<(), ContractMapError> {
        self.active_node
            .take()
            .ok_or(ContractMapError::NoEncounterInProgress)?;
        Ok(())
    }
}

fn graph_contains_cycle(nodes: &BTreeMap<NodeId, EncounterNode>) -> bool {
    let mut incoming_edges: BTreeMap<NodeId, usize> =
        nodes.keys().copied().map(|node_id| (node_id, 0)).collect();

    for node in nodes.values() {
        for next_node in &node.next_nodes {
            *incoming_edges
                .get_mut(next_node)
                .expect("connections were validated before cycle detection") += 1;
        }
    }

    let mut ready: VecDeque<_> = incoming_edges
        .iter()
        .filter_map(|(node_id, count)| (*count == 0).then_some(*node_id))
        .collect();
    let mut visited = 0;

    while let Some(node_id) = ready.pop_front() {
        visited += 1;
        let node = nodes
            .get(&node_id)
            .expect("cycle traversal only queues existing nodes");
        for next_node in &node.next_nodes {
            let count = incoming_edges
                .get_mut(next_node)
                .expect("connections were validated before cycle detection");
            *count -= 1;
            if *count == 0 {
                ready.push_back(*next_node);
            }
        }
    }

    visited != nodes.len()
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::SeedableRng;
    use rand::rngs::StdRng;

    fn branching_map() -> ContractMap {
        ContractMap::new(
            [
                EncounterNode::new(0, EncounterType::Entry, vec![1, 2]),
                EncounterNode::new(1, EncounterType::Combat, vec![3]),
                EncounterNode::new(2, EncounterType::Event, vec![]),
                EncounterNode::new(3, EncounterType::Elite, vec![2]),
            ],
            0,
        )
        .unwrap()
    }

    #[test]
    fn every_reachable_next_encounter_is_selectable_with_its_type() {
        let contract = branching_map();

        let selectable: Vec<_> = contract
            .selectable_encounters()
            .into_iter()
            .map(|node| (node.id(), node.encounter_type()))
            .collect();

        assert_eq!(
            selectable,
            vec![(1, EncounterType::Combat), (2, EncounterType::Event)]
        );
    }

    #[test]
    fn selecting_starts_an_encounter_and_completion_advances_to_it() {
        let mut contract = branching_map();

        let selected = contract.select_encounter(1).unwrap();

        assert_eq!(selected.node_id, 1);
        assert_eq!(selected.encounter_type, EncounterType::Combat);
        assert_eq!(contract.current_encounter().id(), 0);
        assert!(contract.selectable_encounters().is_empty());

        contract.complete_current_encounter().unwrap();
        assert_eq!(contract.current_encounter().id(), 1);
    }

    #[test]
    fn unchosen_branch_stays_locked_even_if_a_later_node_links_to_it() {
        let mut contract = branching_map();
        contract.select_encounter(1).unwrap();
        contract.complete_current_encounter().unwrap();
        contract.select_encounter(3).unwrap();
        contract.complete_current_encounter().unwrap();

        assert!(contract.selectable_encounters().is_empty());
        assert_eq!(
            contract.select_encounter(2),
            Err(ContractMapError::EncounterNotReachable {
                from: 3,
                requested: 2,
            })
        );
    }

    #[test]
    fn locked_or_non_adjacent_selection_does_not_mutate_the_contract() {
        let mut contract = branching_map();
        let before = contract.clone();

        assert!(matches!(
            contract.select_encounter(3),
            Err(ContractMapError::EncounterNotReachable { .. })
        ));
        assert_eq!(contract, before);
    }

    #[test]
    fn another_route_cannot_be_chosen_while_an_encounter_is_active() {
        let mut contract = branching_map();
        contract.select_encounter(1).unwrap();

        assert_eq!(
            contract.select_encounter(2),
            Err(ContractMapError::EncounterAlreadyInProgress(1))
        );
    }

    #[test]
    fn invalid_graph_connections_are_rejected() {
        let result = ContractMap::new([EncounterNode::new(0, EncounterType::Entry, vec![99])], 0);

        assert_eq!(
            result,
            Err(ContractMapError::UnknownConnection { from: 0, to: 99 })
        );
    }

    #[test]
    fn duplicate_outgoing_connections_are_rejected() {
        let result = ContractMap::new(
            [
                EncounterNode::new(0, EncounterType::Entry, vec![1, 1]),
                EncounterNode::new(1, EncounterType::Combat, vec![]),
            ],
            0,
        );

        assert_eq!(
            result,
            Err(ContractMapError::DuplicateConnection { from: 0, to: 1 })
        );
    }

    #[test]
    fn cyclic_contract_graphs_are_rejected() {
        let result = ContractMap::new(
            [
                EncounterNode::new(0, EncounterType::Entry, vec![1]),
                EncounterNode::new(1, EncounterType::Combat, vec![2]),
                EncounterNode::new(2, EncounterType::Event, vec![1]),
            ],
            0,
        );

        assert_eq!(result, Err(ContractMapError::CyclicGraph));
    }

    #[test]
    fn failed_encounter_can_be_retried_without_unlocking_its_sibling() {
        let mut contract = branching_map();
        contract.select_encounter(1).unwrap();

        contract.fail_current_encounter().unwrap();

        let selectable: Vec<_> = contract
            .selectable_encounters()
            .into_iter()
            .map(|node| node.id())
            .collect();
        assert_eq!(contract.current_encounter().id(), 0);
        assert_eq!(selectable, vec![1]);
    }
    fn status_of(contract: &ContractMap, node_id: NodeId) -> NodeStatus {
        contract
            .node_progress()
            .into_iter()
            .find(|entry| entry.node_id == node_id)
            .expect("node exists on the contract")
            .status
    }

    /// Acceptance scenario 3: a brand-new run has nothing completed.
    #[test]
    fn a_new_contract_has_no_completed_encounters_and_zero_progress() {
        let contract = ContractMap::demo();

        let progress = contract.progress();
        assert_eq!(progress.completed, 0);
        assert_eq!(
            progress.total, 3,
            "a route through the demo contract is 3 encounters after the entry"
        );

        assert!(
            !contract
                .node_progress()
                .iter()
                .any(|entry| entry.status == NodeStatus::Completed),
            "nothing should be marked completed before anything is played"
        );
        assert_eq!(status_of(&contract, 0), NodeStatus::Current);
    }

    /// Acceptance scenario 1: completed encounters are marked, and the rest
    /// are visibly in a different state.
    #[test]
    fn completing_an_encounter_marks_it_and_leaves_others_distinct() {
        let mut contract = ContractMap::demo();

        contract.select_encounter(1).expect("node 1 is reachable");
        contract
            .complete_current_encounter()
            .expect("an encounter is active");

        assert_eq!(status_of(&contract, 1), NodeStatus::Completed);
        assert_eq!(
            status_of(&contract, 2),
            NodeStatus::Locked,
            "the branch not taken is locked out"
        );
        assert_eq!(
            status_of(&contract, 3),
            NodeStatus::Available,
            "the next encounter on the chosen route can be selected"
        );
        assert_eq!(
            status_of(&contract, 5),
            NodeStatus::Upcoming,
            "later encounters are not reachable yet"
        );
    }

    /// A node that could only have been reached through a branch the player
    /// passed over is gone for good, however far ahead it sits. Telling them
    /// it is "further ahead on the contract" is the confusion the statuses
    /// exist to prevent.
    #[test]
    fn nodes_behind_a_locked_branch_are_locked_too() {
        let mut contract = ContractMap::demo();

        contract.select_encounter(1).expect("node 1 is reachable");
        contract
            .complete_current_encounter()
            .expect("an encounter is active");

        assert_eq!(
            status_of(&contract, 2),
            NodeStatus::Locked,
            "the branch passed over"
        );
        assert_eq!(
            status_of(&contract, 4),
            NodeStatus::Locked,
            "node 4 is only reachable through node 2, so it is unreachable too"
        );
        assert_eq!(
            status_of(&contract, 3),
            NodeStatus::Available,
            "the route actually taken stays open"
        );
        assert_eq!(
            status_of(&contract, 5),
            NodeStatus::Upcoming,
            "the target is still ahead on the chosen route"
        );
    }

    /// Walks the zoned demo up to, but not into, zone 1's boss.
    fn standing_before_the_first_boss() -> ContractMap {
        let mut contract = ContractMap::demo_zoned();
        contract.select_encounter(1).expect("node 1 is reachable");
        contract
            .complete_current_encounter()
            .expect("an encounter is active");
        contract
    }

    /// Card scenario 2, first half: until the boss goes down, the zone behind
    /// it is sealed and none of it can be entered.
    #[test]
    fn the_next_zone_stays_sealed_until_the_boss_is_beaten() {
        let contract = standing_before_the_first_boss();

        assert_eq!(
            contract.zone_progress(),
            ZoneProgress {
                current_zone: 0,
                unlocked_zones: 0,
                total_zones: 2,
            }
        );
        for node_id in [4, 5, 6] {
            assert_eq!(
                status_of(&contract, node_id),
                NodeStatus::Sealed,
                "node {node_id} is in zone 2 and the boss is still standing"
            );
        }
        assert_eq!(
            contract
                .selectable_encounters()
                .iter()
                .map(|node| node.id)
                .collect::<Vec<_>>(),
            vec![3],
            "the boss is the only way on"
        );
    }

    /// Card scenario 2, second half: beating the boss opens the next zone and
    /// its nodes become ones the player can actually enter.
    #[test]
    fn beating_the_boss_unlocks_the_next_zone_and_offers_its_nodes() {
        let mut contract = standing_before_the_first_boss();

        contract.select_encounter(3).expect("the boss is reachable");
        contract
            .complete_current_encounter()
            .expect("the boss encounter is active");

        assert_eq!(
            contract.zone_progress(),
            ZoneProgress {
                // The boss closes zone 1, so standing on it is still standing
                // in the zone it guarded; the next zone is now open ahead.
                current_zone: 0,
                unlocked_zones: 1,
                total_zones: 2,
            },
            "clearing the boss opens the zone it was guarding"
        );

        let offered: Vec<NodeId> = contract
            .selectable_encounters()
            .iter()
            .map(|node| node.id)
            .collect();
        assert_eq!(
            offered,
            vec![4, 5],
            "both of the next zone's opening nodes are enterable"
        );
        for node_id in [4, 5] {
            assert_eq!(
                status_of(&contract, node_id),
                NodeStatus::Available,
                "node {node_id} should read as somewhere the player can go"
            );
        }
        assert!(
            !contract
                .node_progress()
                .iter()
                .any(|entry| entry.status == NodeStatus::Sealed),
            "nothing is left sealed once the only boss in the way is beaten"
        );

        // And stepping into it is what actually moves the player on a zone.
        contract.select_encounter(4).expect("node 4 is now open");
        contract
            .complete_current_encounter()
            .expect("an encounter is active");
        assert_eq!(contract.zone_progress().current_zone, 1);
    }

    /// The gate is a rule, not merely a shape of graph. A contract that links
    /// straight past its boss still cannot be walked past it: the request is
    /// refused, it says which zone is in the way, and nothing moves.
    #[test]
    fn an_edge_that_bypasses_the_boss_is_still_refused() {
        let contract = ContractMap::new(
            [
                // Node 2 sits in zone 2 but the entry links directly to it,
                // going around the boss entirely.
                EncounterNode::new(0, EncounterType::Entry, vec![1, 2]),
                EncounterNode::new(1, EncounterType::Boss, vec![2]),
                EncounterNode::new(2, EncounterType::Combat, vec![]),
            ],
            0,
        )
        .expect("the bypass graph is valid");
        let before = contract.clone();

        assert_eq!(
            contract.encounter_option(2),
            Err(ContractMapError::ZoneSealed {
                requested: 2,
                zone: 1
            }),
            "the zone gate holds even where the graph offers a way round"
        );
        assert_eq!(contract, before, "a refused entry changes nothing");
        assert_eq!(
            contract
                .selectable_encounters()
                .iter()
                .map(|node| node.id)
                .collect::<Vec<_>>(),
            vec![1],
            "only the boss is on offer"
        );
        assert_eq!(
            status_of(&contract, 2),
            NodeStatus::Sealed,
            "and the bypassed node reads as sealed rather than available"
        );
        assert!(
            ContractMapError::ZoneSealed {
                requested: 2,
                zone: 1
            }
            .to_string()
            .contains("zone 2"),
            "the message counts zones the way the player does"
        );
    }

    /// Failing the boss must not open the zone it was guarding: only beating
    /// it does.
    #[test]
    fn a_failed_boss_encounter_leaves_the_next_zone_sealed() {
        let mut contract = standing_before_the_first_boss();

        contract.select_encounter(3).expect("the boss is reachable");
        contract
            .fail_current_encounter()
            .expect("the boss encounter is active");

        assert_eq!(contract.zone_progress().unlocked_zones, 0);
        assert_eq!(status_of(&contract, 4), NodeStatus::Sealed);
        assert_eq!(
            contract
                .selectable_encounters()
                .iter()
                .map(|node| node.id)
                .collect::<Vec<_>>(),
            vec![3],
            "the boss is still the only way on, and can be retried"
        );
    }

    /// Being locked out by your own branch choice is permanent, so it is the
    /// more useful thing to show even where the zone gate also applies.
    #[test]
    fn a_node_ruled_out_by_an_earlier_choice_reads_as_locked_not_sealed() {
        let mut contract = ContractMap::demo_zoned();
        contract.select_encounter(1).expect("node 1 is reachable");
        contract
            .complete_current_encounter()
            .expect("an encounter is active");

        assert_eq!(
            status_of(&contract, 2),
            NodeStatus::Locked,
            "the branch passed over is gone for good, zones or no zones"
        );
    }

    /// A contract with no boss on it is one open zone, which is what every
    /// authored contract predating zones has to keep being.
    #[test]
    fn a_contract_without_a_boss_is_a_single_unsealed_zone() {
        let contract = ContractMap::demo();

        assert_eq!(
            contract.zone_progress(),
            ZoneProgress {
                current_zone: 0,
                unlocked_zones: 0,
                total_zones: 1,
            }
        );
        assert!(
            !contract
                .node_progress()
                .iter()
                .any(|entry| entry.status == NodeStatus::Sealed),
            "there is no boss to seal anything behind"
        );
    }

    /// Card scenario 1, structural half: the player reaches a boss at the end
    /// of every zone, and it is always a column of its own.
    #[test]
    fn a_generated_contract_ends_every_zone_with_one_boss() {
        let shape = ContractShape::default();
        for seed in 0..25 {
            let contract = generated(seed);
            let bosses: Vec<NodeProgress> = contract
                .node_progress()
                .into_iter()
                .filter(|entry| entry.is_boss)
                .collect();

            assert_eq!(
                bosses.len(),
                shape.zone_count,
                "seed {seed} should field one boss per zone"
            );
            for boss in &bosses {
                assert_eq!(
                    contract
                        .node_progress()
                        .iter()
                        .filter(|entry| entry.x == boss.x)
                        .count(),
                    1,
                    "seed {seed}: the boss at zone {} shares its column",
                    boss.zone
                );
            }

            let mut zones: Vec<usize> = bosses.iter().map(|boss| boss.zone).collect();
            zones.sort_unstable();
            assert_eq!(
                zones,
                (0..shape.zone_count).collect::<Vec<_>>(),
                "seed {seed} should have exactly one boss closing each zone"
            );
            assert!(
                bosses
                    .iter()
                    .any(|boss| boss.is_target && boss.zone == shape.zone_count - 1),
                "seed {seed}: the last zone's boss is the contract target"
            );
        }
    }

    /// Walking a whole generated contract has to pass through every zone gate,
    /// which is the end-to-end version of the two scenarios above.
    #[test]
    fn walking_a_generated_contract_unlocks_each_zone_in_turn() {
        let shape = ContractShape::default();
        for seed in 0..10 {
            let mut contract = generated(seed);
            let mut bosses_beaten = 0;

            loop {
                let options = contract.selectable_encounters();
                if options.is_empty() {
                    break;
                }
                let chosen = options[0].id;
                let was_boss = options[0].encounter_type.is_boss();
                let unlocked_before = contract.zone_progress().unlocked_zones;

                contract.select_encounter(chosen).expect("selectable");
                contract.complete_current_encounter().expect("active");

                let unlocked_after = contract.zone_progress().unlocked_zones;
                if was_boss {
                    bosses_beaten += 1;
                    assert_eq!(
                        unlocked_after,
                        unlocked_before + 1,
                        "seed {seed}: beating a boss has to open the next zone"
                    );
                } else {
                    assert_eq!(
                        unlocked_after, unlocked_before,
                        "seed {seed}: only a boss opens a zone"
                    );
                }

                // Nothing beyond the zones opened so far is ever on offer.
                for option in contract.selectable_encounters() {
                    let zone = contract.node_zones().get(&option.id).copied().unwrap_or(0);
                    assert!(
                        zone <= unlocked_after,
                        "seed {seed}: node {} in zone {zone} was offered too early",
                        option.id
                    );
                }
            }

            assert_eq!(
                bosses_beaten, shape.zone_count,
                "seed {seed}: a full run goes through every zone's boss"
            );
        }
    }

    /// Deeper contracts strand far more nodes behind a single choice, so the
    /// rule has to hold at depth rather than just on the two-level demo.
    #[test]
    fn a_generated_contract_locks_everything_it_can_no_longer_reach() {
        let mut contract = generated(4);

        let first = contract.selectable_encounters()[0].id;
        contract.select_encounter(first).expect("selectable");
        contract
            .complete_current_encounter()
            .expect("an encounter is active");

        let reachable = contract.reachable_from_current();
        for entry in contract.node_progress() {
            if entry.status == NodeStatus::Completed || entry.node_id == contract.current_node {
                continue;
            }

            let should_be_locked = !reachable.contains(&entry.node_id);
            assert_eq!(
                entry.status == NodeStatus::Locked,
                should_be_locked,
                "node {} is {:?} but reachable={}",
                entry.node_id,
                entry.status,
                !should_be_locked
            );
        }
    }

    /// Nothing is ruled out before the player has chosen anything.
    #[test]
    fn a_fresh_contract_locks_nothing() {
        let contract = ContractMap::demo();

        assert!(
            !contract
                .node_progress()
                .iter()
                .any(|entry| entry.status == NodeStatus::Locked),
            "no branch has been passed over yet"
        );
    }

    /// Acceptance scenario 2: overall progress is countable, e.g. "2 of 5".
    #[test]
    fn progress_counts_completed_encounters_and_excludes_the_entry_node() {
        let mut contract = ContractMap::demo();

        contract.select_encounter(1).expect("node 1 is reachable");
        contract
            .complete_current_encounter()
            .expect("an encounter is active");
        assert_eq!(contract.progress().completed, 1);

        contract.select_encounter(3).expect("node 3 is reachable");
        contract
            .complete_current_encounter()
            .expect("an encounter is active");

        let progress = contract.progress();
        assert_eq!(progress.completed, 2);
        assert_eq!(progress.total, 3);
    }

    /// A failed encounter is not progress, so it must not be counted.
    #[test]
    fn a_failed_encounter_does_not_count_as_completed() {
        let mut contract = ContractMap::demo();

        contract.select_encounter(1).expect("node 1 is reachable");
        contract
            .fail_current_encounter()
            .expect("an encounter is active");

        assert_eq!(contract.progress().completed, 0);
        assert_ne!(status_of(&contract, 1), NodeStatus::Completed);
    }

    #[test]
    fn the_entry_node_reads_as_behind_you_once_the_run_moves_on() {
        let mut contract = ContractMap::demo();
        assert_eq!(status_of(&contract, 0), NodeStatus::Current);

        contract.select_encounter(1).expect("node 1 is reachable");
        contract
            .complete_current_encounter()
            .expect("an encounter is active");

        assert_eq!(
            status_of(&contract, 0),
            NodeStatus::Completed,
            "the entry node is behind the player, not still ahead"
        );
        assert_eq!(
            contract.progress().completed,
            1,
            "the entry node still must not count towards encounters completed"
        );
    }

    /// Card 70 scenario 2: a cleared node is both completed and where the
    /// player stands, and the map has to be able to show both.
    #[test]
    fn the_node_the_player_stands_on_is_marked_even_once_it_is_completed() {
        let mut contract = ContractMap::demo();
        assert!(progress_of(&contract, 0).is_current);

        contract.select_encounter(1).expect("node 1 is reachable");
        contract
            .complete_current_encounter()
            .expect("an encounter is active");

        let standing_on = progress_of(&contract, 1);
        assert!(standing_on.is_current, "the player is standing on node 1");
        assert_eq!(
            standing_on.status,
            NodeStatus::Completed,
            "and has also cleared it"
        );
        assert!(
            !progress_of(&contract, 0).is_current,
            "the entry is behind them now"
        );
        assert_eq!(
            contract
                .node_progress()
                .iter()
                .filter(|entry| entry.is_current)
                .count(),
            1,
            "exactly one node is where the player is"
        );
    }

    #[test]
    fn every_node_appears_exactly_once_in_the_progress_view() {
        let contract = ContractMap::demo();

        let entries = contract.node_progress();
        let mut ids: Vec<NodeId> = entries.iter().map(|entry| entry.node_id).collect();
        ids.sort_unstable();

        assert_eq!(ids, vec![0, 1, 2, 3, 4, 5]);
    }
    fn progress_of(contract: &ContractMap, node_id: NodeId) -> NodeProgress {
        contract
            .node_progress()
            .into_iter()
            .find(|entry| entry.node_id == node_id)
            .expect("node exists on the contract")
    }

    /// Card 70 scenario 1: the entry and the target are the two fixed points
    /// of a run, so they sit at either end of the layout.
    #[test]
    fn the_layout_runs_from_the_entry_on_the_left_to_the_target_on_the_right() {
        let contract = ContractMap::demo();

        let entry = progress_of(&contract, 0);
        let target = progress_of(&contract, 5);

        assert_eq!(entry.x, 0.0, "the entry starts the layout");
        assert!(entry.is_entry);
        assert_eq!(target.x, 1.0, "the target ends it");
        assert!(target.is_target);

        for entry in contract.node_progress() {
            assert!(
                (0.0..=1.0).contains(&entry.x) && (0.0..=1.0).contains(&entry.y),
                "node {} sits outside the layout at ({}, {})",
                entry.node_id,
                entry.x,
                entry.y
            );
        }
    }

    /// A node every branch reconverges on belongs in the final column, not
    /// pulled left by whichever route reached it first.
    #[test]
    fn a_reconverging_node_sits_past_every_branch_that_leads_to_it() {
        let contract = ContractMap::demo();

        let target = progress_of(&contract, 5);
        for node_id in [1, 2, 3, 4] {
            let node = progress_of(&contract, node_id);
            assert!(
                node.x < target.x,
                "node {node_id} should sit left of the target it leads to"
            );
        }
    }

    /// Two routes running in parallel have to be drawn apart, or the branch
    /// reads as a single line.
    #[test]
    fn nodes_in_the_same_column_are_spread_apart() {
        let contract = ContractMap::demo();

        let first_branch = progress_of(&contract, 1);
        let second_branch = progress_of(&contract, 2);

        assert_eq!(
            first_branch.x, second_branch.x,
            "both opening branches are one step from the entry"
        );
        assert_ne!(
            first_branch.y, second_branch.y,
            "but they must not be drawn on top of each other"
        );
    }

    #[test]
    fn no_two_nodes_share_a_position() {
        let contract = ContractMap::demo();

        let mut seen: Vec<(u32, u32)> = Vec::new();
        for entry in contract.node_progress() {
            // Compare as fixed-point, since positions are computed floats.
            let point = ((entry.x * 1000.0) as u32, (entry.y * 1000.0) as u32);
            assert!(
                !seen.contains(&point),
                "node {} overlaps another node at {point:?}",
                entry.node_id
            );
            seen.push(point);
        }
    }

    /// The lines between nodes are drawn from these, so every connection in
    /// the graph has to survive into the layout.
    #[test]
    fn every_connection_is_carried_through_for_drawing() {
        let contract = ContractMap::demo();

        let entry = progress_of(&contract, 0);
        assert_eq!(entry.connections, vec![1, 2]);

        let target = progress_of(&contract, 5);
        assert!(
            target.connections.is_empty(),
            "nothing leads on from the target"
        );

        let converging = progress_of(&contract, 3);
        assert_eq!(converging.connections, vec![5]);
    }

    #[test]
    fn the_layout_does_not_move_as_the_run_progresses() {
        let mut contract = ContractMap::demo();
        let before: Vec<(NodeId, u32, u32)> = contract
            .node_progress()
            .into_iter()
            .map(|e| (e.node_id, (e.x * 1000.0) as u32, (e.y * 1000.0) as u32))
            .collect();

        contract.select_encounter(1).expect("node 1 is reachable");
        contract
            .complete_current_encounter()
            .expect("an encounter is active");

        let after: Vec<(NodeId, u32, u32)> = contract
            .node_progress()
            .into_iter()
            .map(|e| (e.node_id, (e.x * 1000.0) as u32, (e.y * 1000.0) as u32))
            .collect();

        assert_eq!(before, after, "nodes must not jump around mid-run");
    }
    fn generated(seed: u64) -> ContractMap {
        let mut rng = StdRng::seed_from_u64(seed);
        ContractMap::generate(ContractShape::default(), &mut rng)
    }

    /// The whole point of generating: a contract long enough to be a run.
    #[test]
    fn a_generated_contract_is_as_long_as_it_was_asked_to_be() {
        for seed in 0..25 {
            let contract = generated(seed);
            let depth = contract.node_depths().values().copied().max().unwrap_or(0);
            assert_eq!(
                depth + 1,
                ContractShape::default().length(),
                "seed {seed} produced a contract of the wrong length"
            );
        }
    }

    /// Whichever branches the player commits to, the route has to keep going
    /// until it reaches the target. This walks a full run to prove it.
    #[test]
    fn every_route_through_a_generated_contract_reaches_the_target() {
        for seed in 0..25 {
            let mut contract = generated(seed);
            let mut steps = 0;

            loop {
                let options = contract.selectable_encounters();
                if options.is_empty() {
                    break;
                }
                // Take whichever branch is offered first; any of them must do.
                let chosen = options[0].id;
                contract
                    .select_encounter(chosen)
                    .expect("an offered encounter must be selectable");
                contract
                    .complete_current_encounter()
                    .expect("an encounter is active");
                steps += 1;
                assert!(steps < 50, "seed {seed} never reached the target");
            }

            let ended_on = contract.current_encounter();
            assert!(
                ended_on.next_nodes.is_empty(),
                "seed {seed} stopped at node {} without reaching a target",
                ended_on.id
            );
            assert_eq!(
                steps + 1,
                ContractShape::default().length(),
                "seed {seed} traversed the wrong number of encounters"
            );
        }
    }

    /// A finished contract has to read as finished, not as a fraction of a
    /// graph the player was never able to walk all of.
    #[test]
    fn walking_a_generated_contract_to_the_end_completes_its_progress() {
        let mut contract = generated(3);

        loop {
            let options = contract.selectable_encounters();
            if options.is_empty() {
                break;
            }
            let chosen = options[0].id;
            contract.select_encounter(chosen).expect("selectable");
            contract.complete_current_encounter().expect("active");
        }

        let progress = contract.progress();
        assert_eq!(
            progress.completed, progress.total,
            "reaching the target should read as {} of {} complete",
            progress.total, progress.total
        );
        assert_eq!(
            progress.total,
            ContractShape::default().length() - 1,
            "a route is one encounter per column after the entry"
        );
    }

    #[test]
    fn a_generated_contract_has_one_entry_and_one_target() {
        for seed in 0..25 {
            let contract = generated(seed);

            let entries: Vec<_> = contract
                .node_progress()
                .into_iter()
                .filter(|node| node.is_entry)
                .collect();
            assert_eq!(
                entries.len(),
                1,
                "seed {seed} has {} entries",
                entries.len()
            );

            let targets: Vec<_> = contract
                .node_progress()
                .into_iter()
                .filter(|node| node.is_target)
                .collect();
            assert_eq!(
                targets.len(),
                1,
                "seed {seed} has {} targets",
                targets.len()
            );
        }
    }

    /// A node with no way in cannot be reached, and one with no way out is a
    /// dead end the player could get stranded on.
    #[test]
    fn no_generated_node_is_stranded() {
        for seed in 0..25 {
            let contract = generated(seed);

            for node in contract.nodes.values() {
                if !node.next_nodes.is_empty() {
                    continue;
                }
                assert!(
                    contract
                        .node_progress()
                        .into_iter()
                        .find(|entry| entry.node_id == node.id)
                        .expect("node exists")
                        .is_target,
                    "seed {seed}: node {} is a dead end but is not the target",
                    node.id
                );
            }

            for node in contract.nodes.values() {
                if node.id == contract.start_node {
                    continue;
                }
                let has_way_in = contract
                    .nodes
                    .values()
                    .any(|other| other.next_nodes.contains(&node.id));
                assert!(has_way_in, "seed {seed}: nothing leads to node {}", node.id);
            }
        }
    }

    #[test]
    fn generating_offers_branches_to_choose_between() {
        let contract = generated(1);
        let branch_points = contract
            .nodes
            .values()
            .filter(|node| node.next_nodes.len() > 1)
            .count();
        assert!(
            branch_points > 0,
            "a contract with no branches is a corridor, not a map"
        );
    }

    #[test]
    fn the_same_seed_generates_the_same_contract() {
        assert_eq!(generated(7), generated(7));
    }

    #[test]
    fn different_seeds_generate_different_contracts() {
        let first = generated(1);
        let second = generated(2);
        assert_ne!(
            first, second,
            "two runs should not lay out the same contract"
        );
    }
}
