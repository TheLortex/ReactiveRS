use super::network::CrossroadId;
use super::road::RoadId;

/// Node identifier
pub type NodeId = usize;

/// Edge Identifier
pub type EdgeId = usize;

/// Node information. This corresponds to a Crossroad identifier.
pub type NodeInfo = CrossroadId;

/// Edge information. This corresponds to a Road identifier.
pub type EdgeInfo = RoadId;

/// Node of the graph. This corresponds to a quarter of a crossroad.
#[derive(Clone)]
pub struct Node {
    id: NodeId,         // Node identifier.
    info: NodeInfo,     // Corresponding crossroad identifier.
    edges: Vec<EdgeId>, // Edges accesible from this node.
}

/// Edge of the graph. This corresponds to a road.
#[derive(Copy, Clone)]
pub struct Edge {
    id: EdgeId,             // Edge identifier.
    info: EdgeInfo,         // Corresponding Road identifier.
    source: NodeId,         // Source node of the edge.
    destination: NodeId,    // Destination node of the edge.
}

/// Graph structure.
#[derive(Clone)]
pub struct Graph {
    node_count: usize,
    edge_count: usize,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

/// Weight for edges.
pub type Weight = f32;

/// Structure to save the weights of the edges.
pub struct EdgesWeight {
    pub weights: Vec<Weight>,
}

impl Node {
    /// Creates a new node from the specified information.
    pub fn new(id: NodeId, info: NodeInfo) -> Node {
        Node { id, info, edges: vec!() }
    }

    /// Adds an edge to the node.
    pub fn add_edge(&mut self, edge: EdgeId) {
        self.edges.push(edge);
    }

    /// Returns the identifier of the node.
    pub fn id(&self) -> NodeId {
        self.id
    }

    /// Returns the information of the node.
    pub fn info(&self) -> &NodeInfo {
        &self.info
    }

    /// Returns the edges that are accessible from the node.
    pub fn edges(&self) -> &Vec<EdgeId> {
        &self.edges
    }
}

impl Edge {
    /// Creates a new edge from the specified information.
    pub fn new(id: EdgeId, info: EdgeInfo, source: NodeId, destination: NodeId) -> Edge {
        Edge { id, info, source, destination}
    }

    /// Returns the identifier of the edge.
    pub fn id(&self) -> EdgeId {
        self.id
    }

    /// Returns the information of the edge.s
    pub fn info(&self) -> &EdgeInfo {
        &self.info
    }

    /// Returns the source node of the edge.
    pub fn source(&self) -> NodeId {
        self.source
    }

    /// Returns the destination node of the edge.
    pub fn destination(&self) -> NodeId {
        self.destination
    }
}


impl Graph {
    /// Returns a new empty graph.
    pub fn new() -> Graph {
        Graph { node_count: 0, edge_count: 0, nodes: vec!(), edges: vec!()}
    }

    /// Adds a node with corresponding information, and returns the created Node Identifier.
    pub fn add_node(&mut self, info: NodeInfo) -> NodeId {
        let node = Node::new(self.node_count, info);
        let id = node.id();
        // We create a fresh node identifier.
        self.node_count += 1;
        self.nodes.push(node);
        id
    }

    /// Adds an edge in the graph between the specified nodes, with the specified information.
    pub fn add_edge(&mut self, source: NodeId, destination: NodeId, info: EdgeInfo) {
        let edge_id = self.edge_count;
        let edge = Edge::new(edge_id, info, source, destination);
        // We create a fresh edge identifier.
        self.edge_count += 1;
        self.edges.push(edge);
        self.nodes[source].add_edge(edge_id);
    }

    /// Returns the specified node.
    pub fn get_node(&self, node: NodeId) -> &Node {
        &self.nodes[node]
    }

    /// Returns the specified edge.
    pub fn get_edge(&self, edge: EdgeId) -> &Edge {
        &self.edges[edge]
    }
}

use std::fmt;
impl fmt::Display for Graph {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        for node in &self.nodes {
            let _ = write!(f, "Node {}:", node.id());
            for &edge_id in node.edges() {
                let _ = write!(f, " {}", self.get_edge(edge_id).destination());
            }
            let _ = write!(f, "\n");
        }
        write!(f, "")
    }
}

impl EdgesWeight {
    /// Creates a new weights structure with the specified edges.
    pub fn new(weights: Vec<Weight>) -> EdgesWeight {
        EdgesWeight { weights }
    }

    /// Returns the index corresponding to the specified edge.
    /// Here, this index is equal to the EdgeInfo.
    pub fn get_index(&self, edge: EdgeInfo) -> usize {
        edge as usize
    }

    /// Returns the weight of the specified edge.
    pub fn get_weight(&self, edge: &Edge) -> Weight {
        self.weights[self.get_index(*edge.info())]
    }
}


/*
    Dijkstra Implementation
    (Largely inspired from the Dijkstra example in rust documentation (std::collection::binary_heap)
*/

use std::cmp::Ordering;
use std::collections::BinaryHeap;
use std::f32;

/// Couple (Weight, Node) to store in the heap of the Dijkstra algorithm.
#[derive(Copy, Clone, PartialEq)]
struct State {
    cost: Weight,
    node: NodeId,
}

impl Eq for State {}

impl Ord for State {
    fn cmp(&self, other: &Self) -> Ordering {
        let ord = self.partial_cmp(other).unwrap();
        // We reverse the order to make the heap become a min heap.
        match ord {
            Ordering::Greater => Ordering::Less,
            Ordering::Less => Ordering::Greater,
            Ordering::Equal => ord,
        }
    }
}

// `PartialOrd` needs to be implemented as well.
impl PartialOrd for State {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        other.cost.partial_cmp(&self.cost)
    }
}

/// Dijkstra algorithm.
pub fn dijkstra<F>(source: NodeId, f: F, graph: &Graph, weights: &EdgesWeight) ->
    (Vec<EdgeId>, Weight) where F: Fn(&NodeInfo) -> bool + Sized
{
    let n = graph.node_count;
    let mut distances: Vec<Weight> = (0..n).map(|_| f32::MAX).collect();
    let mut ancestors: Vec<(usize, usize)> = (0..n).map(|_| (0, 0)).collect();
    let mut heap = BinaryHeap::new();
    let mut dest = source;

    distances[source] = 0.;
    ancestors[source] = (0, 0);
    heap.push(State { cost: 0., node: source });

    while let Some(State { cost, node }) = heap.pop() {
        // Alternatively we could have continued to find all shortest paths
        if f(graph.get_node(node).info()) {
            dest = node;
            break;
        }

        // Important as we may have already found a better way
        if cost > distances[node] {
            continue;
        }

        // For each node we can reach, see if we can find a way with
        // a lower cost going through this node
        for edge_id in graph.get_node(node).edges() {
            let edge = graph.get_edge(*edge_id);
            let next = State { cost: cost + weights.get_weight(edge), node: edge.destination() };
            // If so, add it to the frontier and continue
            if next.cost < distances[next.node] {
                heap.push(next);
                // Relaxation, we have now found a better way
                distances[next.node] = next.cost;
                ancestors[next.node] = (node, *edge_id);
            }
        }
    }

    let cost = distances[dest];
    let mut path = vec!();
    while dest != source {
        let (new_dest, edge) = ancestors[dest];
        dest = new_dest;
        path.push(edge);
    }

    if path.is_empty() {
        if f(graph.get_node(source).info()) {
            println!("The source is a wanted destination.");
        }
        println!("The path from {} to crossroad ?? is empty.", source);
    }
    return (path, cost);
}

#[test]
fn test_dijkstra() {
    let mut graph = Graph::new();
    graph.add_node(CrossroadId::new(0, 0));
    graph.add_node(CrossroadId::new(0, 1));
    graph.add_node(CrossroadId::new(0, 2));
    graph.add_node(CrossroadId::new(0, 3));
    graph.add_node(CrossroadId::new(0, 4));

    graph.add_edge(0, 2, 0);
    graph.add_edge(0, 1, 1);
    graph.add_edge(1, 3, 2);
    graph.add_edge(2, 1, 3);
    graph.add_edge(2, 3, 4);
    graph.add_edge(2, 4, 5);
    graph.add_edge(3, 0, 6);
    graph.add_edge(3, 4, 7);

    let mut weights = EdgesWeight::new(vec![10., 1., 2., 1., 3., 1., 7., 2.]);

    let (_, v) = dijkstra(0, |x| {*x == CrossroadId::new(0, 1) }, &graph, &weights);
    assert_eq!(v, 1.);

    let (_, v) = dijkstra(0, |x| {*x == CrossroadId::new(0, 3) }, &graph, &weights);
    assert_eq!(v, 3.);

    let (_, v) = dijkstra(3, |x| {*x == CrossroadId::new(0, 0) }, &graph, &weights);
    assert_eq!(v, 7.);

    let (_, v) = dijkstra(0, |x| {*x == CrossroadId::new(0, 4) }, &graph, &weights);
    assert_eq!(v, 5.);
}