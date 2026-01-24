//! Routing table for caching route information
//!
//! The [`RoutingTable`] caches route information to avoid
//! repeatedly computing routes to the same destination.
//!
//! Routes have a staleness timeout - after this period without
//! confirmation, the route should be refreshed.

use std::time::Duration;

use chrono::{DateTime, Utc};
use dashmap::DashMap;
use indras_core::{PeerIdentity, routing::RouteInfo};

/// Cached routing table
///
/// Maps destinations to their route information, with automatic
/// staleness detection.
pub struct RoutingTable<I: PeerIdentity> {
    /// Routes indexed by destination (as bytes for key)
    routes: DashMap<Vec<u8>, RouteEntry<I>>,
    /// Duration after which routes are considered stale
    stale_timeout: Duration,
}

/// A route entry with metadata
#[derive(Debug, Clone)]
struct RouteEntry<I: PeerIdentity> {
    /// The route information
    info: RouteInfo<I>,
    /// When this entry was inserted/updated
    inserted_at: DateTime<Utc>,
}

impl<I: PeerIdentity> RoutingTable<I> {
    /// Create a new routing table with the given stale timeout
    ///
    /// # Arguments
    /// * `stale_timeout` - Duration after which routes are considered stale
    pub fn new(stale_timeout: Duration) -> Self {
        Self {
            routes: DashMap::new(),
            stale_timeout,
        }
    }

    /// Insert or update a route to a destination
    pub fn insert(&self, dest: &I, info: RouteInfo<I>) {
        let key = dest.as_bytes();
        let entry = RouteEntry {
            info,
            inserted_at: Utc::now(),
        };
        self.routes.insert(key, entry);
    }

    /// Get the route to a destination
    ///
    /// Returns `None` if no route is cached.
    pub fn get(&self, dest: &I) -> Option<RouteInfo<I>> {
        let key = dest.as_bytes();
        self.routes.get(&key).map(|entry| entry.info.clone())
    }

    /// Remove the route to a destination
    pub fn remove(&self, dest: &I) {
        let key = dest.as_bytes();
        self.routes.remove(&key);
    }

    /// Check if the route to a destination is stale
    ///
    /// A route is stale if:
    /// - It doesn't exist
    /// - It was inserted longer ago than the stale timeout
    /// - It was never confirmed and is older than stale timeout
    pub fn is_stale(&self, dest: &I) -> bool {
        let key = dest.as_bytes();
        match self.routes.get(&key) {
            None => true,
            Some(entry) => {
                let age = Utc::now() - entry.inserted_at;
                let stale_duration =
                    chrono::Duration::from_std(self.stale_timeout).unwrap_or(chrono::Duration::MAX);
                age > stale_duration
            }
        }
    }

    /// Prune all stale routes from the table
    pub fn prune_stale(&self) {
        let stale_duration =
            chrono::Duration::from_std(self.stale_timeout).unwrap_or(chrono::Duration::MAX);
        let now = Utc::now();

        self.routes.retain(|_, entry| {
            let age = now - entry.inserted_at;
            age <= stale_duration
        });
    }

    /// Confirm a route is still valid (updates timestamp)
    ///
    /// This should be called when a delivery confirmation is received.
    pub fn confirm(&self, dest: &I) {
        let key = dest.as_bytes();
        if let Some(mut entry) = self.routes.get_mut(&key) {
            entry.inserted_at = Utc::now();
            entry.info.confirm();
        }
    }

    /// Get the number of cached routes
    pub fn len(&self) -> usize {
        self.routes.len()
    }

    /// Check if the table is empty
    pub fn is_empty(&self) -> bool {
        self.routes.is_empty()
    }

    /// Clear all routes
    pub fn clear(&self) {
        self.routes.clear();
    }

    /// Get all destinations with cached routes
    pub fn destinations(&self) -> Vec<I> {
        self.routes
            .iter()
            .filter_map(|entry| I::from_bytes(&entry.key().clone()).ok())
            .collect()
    }

    /// Update the metric for a route
    pub fn update_metric(&self, dest: &I, metric: u32) {
        let key = dest.as_bytes();
        if let Some(mut entry) = self.routes.get_mut(&key) {
            entry.info.metric = metric;
        }
    }

    /// Get routes sorted by metric (best first)
    pub fn routes_by_metric(&self) -> Vec<RouteInfo<I>> {
        let mut routes: Vec<_> = self.routes.iter().map(|e| e.info.clone()).collect();
        routes.sort_by_key(|r| r.metric);
        routes
    }
}

impl<I: PeerIdentity> Default for RoutingTable<I> {
    fn default() -> Self {
        // Default 5 minute stale timeout
        Self::new(Duration::from_secs(300))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use indras_core::SimulationIdentity;

    fn make_id(c: char) -> SimulationIdentity {
        SimulationIdentity::new(c).unwrap()
    }

    fn make_route(dest: char, next_hop: char, hop_count: u32) -> RouteInfo<SimulationIdentity> {
        RouteInfo::new(make_id(dest), make_id(next_hop), hop_count)
    }

    #[test]
    fn test_insert_and_get() {
        let table: RoutingTable<SimulationIdentity> = RoutingTable::new(Duration::from_secs(300));

        let dest = make_id('C');
        let route = make_route('C', 'B', 2);

        table.insert(&dest, route.clone());

        let retrieved = table.get(&dest);
        assert!(retrieved.is_some());
        let retrieved = retrieved.unwrap();
        assert_eq!(retrieved.destination, dest);
        assert_eq!(retrieved.next_hop, make_id('B'));
        assert_eq!(retrieved.hop_count, 2);
    }

    #[test]
    fn test_remove() {
        let table: RoutingTable<SimulationIdentity> = RoutingTable::new(Duration::from_secs(300));

        let dest = make_id('C');
        let route = make_route('C', 'B', 2);

        table.insert(&dest, route);
        assert!(table.get(&dest).is_some());

        table.remove(&dest);
        assert!(table.get(&dest).is_none());
    }

    #[test]
    fn test_staleness() {
        let table: RoutingTable<SimulationIdentity> = RoutingTable::new(Duration::from_millis(10));

        let dest = make_id('C');
        let route = make_route('C', 'B', 2);

        table.insert(&dest, route);

        // Not stale immediately
        assert!(!table.is_stale(&dest));

        // Wait for it to become stale
        std::thread::sleep(Duration::from_millis(20));

        assert!(table.is_stale(&dest));
    }

    #[test]
    fn test_prune_stale() {
        let table: RoutingTable<SimulationIdentity> = RoutingTable::new(Duration::from_millis(10));

        let c = make_id('C');
        let d = make_id('D');

        table.insert(&c, make_route('C', 'B', 2));

        // Wait for first route to become stale
        std::thread::sleep(Duration::from_millis(20));

        // Add another route (fresh)
        table.insert(&d, make_route('D', 'B', 3));

        // Prune stale routes
        table.prune_stale();

        // C should be removed, D should remain
        assert!(table.get(&c).is_none());
        assert!(table.get(&d).is_some());
    }

    #[test]
    fn test_confirm_refreshes() {
        let table: RoutingTable<SimulationIdentity> = RoutingTable::new(Duration::from_millis(50));

        let dest = make_id('C');
        let route = make_route('C', 'B', 2);

        table.insert(&dest, route);

        // Wait a bit
        std::thread::sleep(Duration::from_millis(30));

        // Confirm the route (should refresh)
        table.confirm(&dest);

        // Wait more
        std::thread::sleep(Duration::from_millis(30));

        // Should not be stale yet (was refreshed)
        assert!(!table.is_stale(&dest));
    }

    #[test]
    fn test_nonexistent_is_stale() {
        let table: RoutingTable<SimulationIdentity> = RoutingTable::new(Duration::from_secs(300));

        let dest = make_id('Z');
        assert!(table.is_stale(&dest));
    }

    #[test]
    fn test_update_metric() {
        let table: RoutingTable<SimulationIdentity> = RoutingTable::new(Duration::from_secs(300));

        let dest = make_id('C');
        let route = make_route('C', 'B', 2);

        table.insert(&dest, route);

        let before = table.get(&dest).unwrap().metric;
        table.update_metric(&dest, 100);
        let after = table.get(&dest).unwrap().metric;

        assert_ne!(before, after);
        assert_eq!(after, 100);
    }

    #[test]
    fn test_routes_by_metric() {
        let table: RoutingTable<SimulationIdentity> = RoutingTable::new(Duration::from_secs(300));

        let c = make_id('C');
        let d = make_id('D');
        let e = make_id('E');

        let mut route_c = make_route('C', 'B', 5);
        route_c.metric = 50;
        let mut route_d = make_route('D', 'B', 2);
        route_d.metric = 10;
        let mut route_e = make_route('E', 'B', 3);
        route_e.metric = 30;

        table.insert(&c, route_c);
        table.insert(&d, route_d);
        table.insert(&e, route_e);

        let sorted = table.routes_by_metric();
        assert_eq!(sorted.len(), 3);
        assert_eq!(sorted[0].metric, 10); // D
        assert_eq!(sorted[1].metric, 30); // E
        assert_eq!(sorted[2].metric, 50); // C
    }

    #[test]
    fn test_clear() {
        let table: RoutingTable<SimulationIdentity> = RoutingTable::new(Duration::from_secs(300));

        table.insert(&make_id('C'), make_route('C', 'B', 2));
        table.insert(&make_id('D'), make_route('D', 'B', 3));
        table.insert(&make_id('E'), make_route('E', 'B', 4));

        assert_eq!(table.len(), 3);

        table.clear();

        assert_eq!(table.len(), 0);
        assert!(table.is_empty());
    }
}
