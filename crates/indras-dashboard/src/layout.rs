//! Force-directed graph layout algorithm
//!
//! Computes peer positions for visualization using a simple force-directed
//! layout where:
//! - All nodes repel each other (Coulomb-like force)
//! - Connected nodes attract each other (spring force)

use indras_simulation::{Mesh, PeerId};
use std::collections::HashMap;

/// Compute peer positions using force-directed layout
///
/// # Arguments
/// * `mesh` - The network mesh topology
/// * `width` - Canvas width in pixels
/// * `height` - Canvas height in pixels
///
/// # Returns
/// HashMap of peer IDs to (x, y) coordinates
pub fn compute_layout(mesh: &Mesh, width: f64, height: f64) -> HashMap<PeerId, (f64, f64)> {
    let peers: Vec<PeerId> = mesh.peer_ids();
    let n = peers.len();

    if n == 0 {
        return HashMap::new();
    }

    // Initialize positions in a circle
    let mut positions: HashMap<PeerId, (f64, f64)> = HashMap::new();
    for (i, peer) in peers.iter().enumerate() {
        let angle = 2.0 * std::f64::consts::PI * (i as f64) / (n as f64);
        let r = width.min(height) * 0.35;
        positions.insert(
            *peer,
            (
                width / 2.0 + r * angle.cos(),
                height / 2.0 + r * angle.sin(),
            ),
        );
    }

    // For very small graphs, the initial circle layout is often fine
    if n <= 2 {
        return positions;
    }

    // Collect edges for attraction
    let edges: Vec<(PeerId, PeerId)> = mesh.interfaces.keys().copied().collect();

    // Run force simulation for iterations
    let iterations = 100;
    let repulsion_strength = 5000.0;
    let attraction_strength = 0.01;
    let damping = 0.1;
    let min_distance = 1.0;

    for _ in 0..iterations {
        let mut forces: HashMap<PeerId, (f64, f64)> =
            peers.iter().map(|p| (*p, (0.0, 0.0))).collect();

        // Repulsion between all pairs
        for i in 0..n {
            for j in (i + 1)..n {
                let (ax, ay) = positions[&peers[i]];
                let (bx, by) = positions[&peers[j]];
                let dx = bx - ax;
                let dy = by - ay;
                let dist = (dx * dx + dy * dy).sqrt().max(min_distance);
                let force = repulsion_strength / (dist * dist);
                let fx = force * dx / dist;
                let fy = force * dy / dist;

                forces.get_mut(&peers[i]).unwrap().0 -= fx;
                forces.get_mut(&peers[i]).unwrap().1 -= fy;
                forces.get_mut(&peers[j]).unwrap().0 += fx;
                forces.get_mut(&peers[j]).unwrap().1 += fy;
            }
        }

        // Attraction along edges
        for (a, b) in &edges {
            let (ax, ay) = positions[a];
            let (bx, by) = positions[b];
            let dx = bx - ax;
            let dy = by - ay;
            let dist = (dx * dx + dy * dy).sqrt().max(min_distance);
            let force = dist * attraction_strength;
            let fx = force * dx / dist;
            let fy = force * dy / dist;

            forces.get_mut(a).unwrap().0 += fx;
            forces.get_mut(a).unwrap().1 += fy;
            forces.get_mut(b).unwrap().0 -= fx;
            forces.get_mut(b).unwrap().1 -= fy;
        }

        // Apply forces with damping, clamping to bounds
        let padding = 50.0;
        for peer in &peers {
            let (fx, fy) = forces[peer];
            let (x, y) = positions.get_mut(peer).unwrap();
            *x = (*x + fx * damping).clamp(padding, width - padding);
            *y = (*y + fy * damping).clamp(padding, height - padding);
        }
    }

    positions
}

/// Update positions for a single iteration (useful for animated layout)
#[allow(dead_code)] // Reserved for future animated layout feature
pub fn layout_step(
    positions: &mut HashMap<PeerId, (f64, f64)>,
    mesh: &Mesh,
    width: f64,
    height: f64,
) {
    let peers: Vec<PeerId> = mesh.peer_ids();
    let n = peers.len();

    if n <= 1 {
        return;
    }

    let edges: Vec<(PeerId, PeerId)> = mesh.interfaces.keys().copied().collect();

    let repulsion_strength = 5000.0;
    let attraction_strength = 0.01;
    let damping = 0.1;
    let min_distance = 1.0;
    let padding = 50.0;

    let mut forces: HashMap<PeerId, (f64, f64)> = peers.iter().map(|p| (*p, (0.0, 0.0))).collect();

    // Repulsion between all pairs
    for i in 0..n {
        for j in (i + 1)..n {
            if let (Some(&(ax, ay)), Some(&(bx, by))) =
                (positions.get(&peers[i]), positions.get(&peers[j]))
            {
                let dx = bx - ax;
                let dy = by - ay;
                let dist = (dx * dx + dy * dy).sqrt().max(min_distance);
                let force = repulsion_strength / (dist * dist);
                let fx = force * dx / dist;
                let fy = force * dy / dist;

                forces.get_mut(&peers[i]).unwrap().0 -= fx;
                forces.get_mut(&peers[i]).unwrap().1 -= fy;
                forces.get_mut(&peers[j]).unwrap().0 += fx;
                forces.get_mut(&peers[j]).unwrap().1 += fy;
            }
        }
    }

    // Attraction along edges
    for (a, b) in &edges {
        if let (Some(&(ax, ay)), Some(&(bx, by))) = (positions.get(a), positions.get(b)) {
            let dx = bx - ax;
            let dy = by - ay;
            let dist = (dx * dx + dy * dy).sqrt().max(min_distance);
            let force = dist * attraction_strength;
            let fx = force * dx / dist;
            let fy = force * dy / dist;

            forces.get_mut(a).unwrap().0 += fx;
            forces.get_mut(a).unwrap().1 += fy;
            forces.get_mut(b).unwrap().0 -= fx;
            forces.get_mut(b).unwrap().1 -= fy;
        }
    }

    // Apply forces with damping
    for peer in &peers {
        if let Some((fx, fy)) = forces.get(peer) {
            if let Some((x, y)) = positions.get_mut(peer) {
                *x = (*x + fx * damping).clamp(padding, width - padding);
                *y = (*y + fy * damping).clamp(padding, height - padding);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_simulation::from_edges;

    #[test]
    fn test_empty_mesh() {
        let mesh = Mesh::new();
        let positions = compute_layout(&mesh, 800.0, 600.0);
        assert!(positions.is_empty());
    }

    #[test]
    fn test_triangle_layout() {
        let mesh = from_edges(&[('A', 'B'), ('B', 'C'), ('A', 'C')]);
        let positions = compute_layout(&mesh, 800.0, 600.0);

        assert_eq!(positions.len(), 3);
        assert!(positions.contains_key(&PeerId('A')));
        assert!(positions.contains_key(&PeerId('B')));
        assert!(positions.contains_key(&PeerId('C')));

        // Check positions are within bounds
        for (x, y) in positions.values() {
            assert!(*x >= 50.0 && *x <= 750.0);
            assert!(*y >= 50.0 && *y <= 550.0);
        }
    }

    #[test]
    fn test_single_peer() {
        let mut mesh = Mesh::new();
        mesh.add_peer(PeerId('A'));
        let positions = compute_layout(&mesh, 800.0, 600.0);

        assert_eq!(positions.len(), 1);
        // Single peer should be placed somewhere reasonable
        let (x, y) = positions[&PeerId('A')];
        assert!(x > 0.0 && x < 800.0);
        assert!(y > 0.0 && y < 600.0);
    }
}
