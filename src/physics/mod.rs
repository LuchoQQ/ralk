use std::sync::Mutex;

use glam::{Quat, Vec3};
use rapier3d::na::UnitQuaternion;
use rapier3d::prelude::*;

pub use rapier3d::prelude::RigidBodyHandle;

// ---------------------------------------------------------------------------
// Contact event collector (Phase 17)
// ---------------------------------------------------------------------------

/// Implements rapier's `EventHandler` trait to collect world-space positions of
/// new collision contacts during a physics step.
///
/// `Mutex` is required because `EventHandler: Send + Sync`.
struct ContactCollector {
    positions: Mutex<Vec<Vec3>>,
}

impl EventHandler for ContactCollector {
    fn handle_collision_event(
        &self,
        _bodies: &RigidBodySet,
        colliders: &ColliderSet,
        event: CollisionEvent,
        _contact_pair: Option<&ContactPair>,
    ) {
        // Only care about new contacts starting.
        if let CollisionEvent::Started(h1, _h2, _flags) = event {
            if let Some(col) = colliders.get(h1) {
                let pos = col.position().translation.vector;
                if let Ok(mut guard) = self.positions.lock() {
                    guard.push(Vec3::new(pos.x, pos.y, pos.z));
                }
            }
        }
    }

    fn handle_contact_force_event(
        &self,
        _dt: Real,
        _bodies: &RigidBodySet,
        _colliders: &ColliderSet,
        _contact_pair: &ContactPair,
        _total_force_magnitude: Real,
    ) {
        // Not used — we react to collision-started events instead.
    }
}

/// Wraps all rapier3d subsystems needed for a physics simulation.
pub struct PhysicsWorld {
    pub rigid_bodies: RigidBodySet,
    pub colliders: ColliderSet,
    gravity: Vector<Real>,
    integration_parameters: IntegrationParameters,
    physics_pipeline: PhysicsPipeline,
    island_manager: IslandManager,
    broad_phase: DefaultBroadPhase,
    narrow_phase: NarrowPhase,
    impulse_joints: ImpulseJointSet,
    multibody_joints: MultibodyJointSet,
    ccd_solver: CCDSolver,
    /// Used for ray-cast ground detection (player jump).
    query_pipeline: QueryPipeline,
}

impl PhysicsWorld {
    pub fn new() -> Self {
        Self {
            rigid_bodies: RigidBodySet::new(),
            colliders: ColliderSet::new(),
            gravity: vector![0.0, -9.81, 0.0],
            integration_parameters: IntegrationParameters::default(),
            physics_pipeline: PhysicsPipeline::new(),
            island_manager: IslandManager::new(),
            broad_phase: DefaultBroadPhase::new(),
            narrow_phase: NarrowPhase::new(),
            impulse_joints: ImpulseJointSet::new(),
            multibody_joints: MultibodyJointSet::new(),
            ccd_solver: CCDSolver::new(),
            query_pipeline: QueryPipeline::new(),
        }
    }

    /// Advance the simulation by `dt` seconds (should be a fixed timestep).
    pub fn step(&mut self, dt: f32) {
        self.integration_parameters.dt = dt;
        self.physics_pipeline.step(
            &self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            None,
            &(),
            &(),
        );
        self.query_pipeline.update(&self.colliders);
    }

    /// Like `step`, but also collects world-space positions of new collision contacts.
    /// Returns one position per new contact started during this step.
    /// Used by the audio system to play impact sounds.
    pub fn step_and_collect_impacts(&mut self, dt: f32) -> Vec<Vec3> {
        self.integration_parameters.dt = dt;
        let collector = ContactCollector { positions: Mutex::new(Vec::new()) };
        self.physics_pipeline.step(
            &self.gravity,
            &self.integration_parameters,
            &mut self.island_manager,
            &mut self.broad_phase,
            &mut self.narrow_phase,
            &mut self.rigid_bodies,
            &mut self.colliders,
            &mut self.impulse_joints,
            &mut self.multibody_joints,
            &mut self.ccd_solver,
            None,
            &(),
            &collector,
        );
        self.query_pipeline.update(&self.colliders);
        collector.positions.into_inner().unwrap_or_default()
    }

    /// Spawn a dynamic rigid body with a box collider at the given position.
    pub fn add_dynamic_box(
        &mut self,
        position: Vec3,
        half_extents: Vec3,
        restitution: f32,
        friction: f32,
    ) -> (RigidBodyHandle, ColliderHandle) {
        let rb = RigidBodyBuilder::dynamic()
            .translation(vector![position.x, position.y, position.z])
            .build();
        let body_handle = self.rigid_bodies.insert(rb);

        let collider = ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z)
            .restitution(restitution)
            .friction(friction)
            .build();
        let collider_handle = self.colliders.insert_with_parent(
            collider,
            body_handle,
            &mut self.rigid_bodies,
        );

        (body_handle, collider_handle)
    }

    /// Spawn a static (fixed) rigid body with a box collider at the given position.
    pub fn add_static_box(
        &mut self,
        position: Vec3,
        half_extents: Vec3,
        restitution: f32,
        friction: f32,
    ) -> (RigidBodyHandle, ColliderHandle) {
        let rb = RigidBodyBuilder::fixed()
            .translation(vector![position.x, position.y, position.z])
            .build();
        let body_handle = self.rigid_bodies.insert(rb);

        let collider = ColliderBuilder::cuboid(half_extents.x, half_extents.y, half_extents.z)
            .restitution(restitution)
            .friction(friction)
            .build();
        let collider_handle = self.colliders.insert_with_parent(
            collider,
            body_handle,
            &mut self.rigid_bodies,
        );

        (body_handle, collider_handle)
    }

    /// Teleport any body (static, dynamic, kinematic) to a new pose.
    /// Use for animated objects (PropertyAnimator) that need their collider to follow.
    pub fn set_body_pose(&mut self, handle: RigidBodyHandle, position: Vec3, rotation: Quat) {
        if let Some(rb) = self.rigid_bodies.get_mut(handle) {
            let iso = rapier3d::math::Isometry::from_parts(
                vector![position.x, position.y, position.z].into(),
                UnitQuaternion::from_quaternion(
                    rapier3d::na::Quaternion::new(rotation.w, rotation.x, rotation.y, rotation.z),
                ),
            );
            rb.set_position(iso, true);
        }
    }

    /// Sync a kinematic body's pose from a glam transform (call before step).
    pub fn set_kinematic_pose(&mut self, handle: RigidBodyHandle, position: Vec3, rotation: Quat) {
        if let Some(rb) = self.rigid_bodies.get_mut(handle) {
            rb.set_next_kinematic_translation(vector![position.x, position.y, position.z]);
            rb.set_next_kinematic_rotation(UnitQuaternion::from_quaternion(
                rapier3d::na::Quaternion::new(rotation.w, rotation.x, rotation.y, rotation.z),
            ));
        }
    }

    /// Spawn a kinematic capsule for the player character.
    /// Capsule is upright (Y axis), half_height=0.5, radius=0.4 → 1.8 m tall.
    /// Returns the body handle; no ECS entity is created for the player.
    pub fn add_player_capsule(&mut self, position: Vec3) -> RigidBodyHandle {
        let rb = RigidBodyBuilder::dynamic()
            .translation(vector![position.x, position.y, position.z])
            .locked_axes(LockedAxes::ROTATION_LOCKED)
            .linear_damping(0.0)
            .build();
        let body_handle = self.rigid_bodies.insert(rb);
        let collider = ColliderBuilder::capsule_y(0.5, 0.4)
            .friction(0.0)
            .restitution(0.0)
            .build();
        self.colliders.insert_with_parent(collider, body_handle, &mut self.rigid_bodies);
        body_handle
    }

    /// Override the horizontal velocity of a body, keeping its current Y velocity.
    pub fn set_horizontal_velocity(&mut self, handle: RigidBodyHandle, xz: Vec3) {
        if let Some(rb) = self.rigid_bodies.get_mut(handle) {
            let cur_y = rb.linvel().y;
            rb.set_linvel(vector![xz.x, cur_y, xz.z], true);
        }
    }

    /// Read the pose of a dynamic body into glam types (call after step).
    pub fn get_dynamic_pose(&self, handle: RigidBodyHandle) -> Option<(Vec3, Quat)> {
        let rb = self.rigid_bodies.get(handle)?;
        let t = rb.translation();
        let r = rb.rotation();
        let pos = Vec3::new(t.x, t.y, t.z);
        let rot = Quat::from_xyzw(r.i, r.j, r.k, r.w);
        Some((pos, rot))
    }

    /// Ray-cast downward from the player capsule center to detect ground contact.
    ///
    /// `half_height + radius + margin` = 0.5 + 0.4 + 0.15 = 1.05 — if the ray
    /// hits anything within that distance, the capsule is considered grounded.
    pub fn is_grounded(&self, handle: RigidBodyHandle) -> bool {
        let rb = match self.rigid_bodies.get(handle) {
            Some(rb) => rb,
            None => return false,
        };
        let pos = rb.translation();
        let ray = Ray::new(
            Point::new(pos.x, pos.y, pos.z),
            Vector::new(0.0, -1.0, 0.0),
        );
        let filter = QueryFilter::default().exclude_rigid_body(handle);
        self.query_pipeline.cast_ray(
            &self.rigid_bodies,
            &self.colliders,
            &ray,
            1.05,   // half_height(0.5) + radius(0.4) + margin(0.15)
            true,
            filter,
        ).is_some()
    }

    /// Apply an upward impulse to a body (used for jumping).
    pub fn apply_jump_impulse(&mut self, handle: RigidBodyHandle, impulse_y: f32) {
        if let Some(rb) = self.rigid_bodies.get_mut(handle) {
            rb.apply_impulse(vector![0.0, impulse_y, 0.0], true);
        }
    }

    /// Read the current Y velocity of a body.
    pub fn get_y_velocity(&self, handle: RigidBodyHandle) -> f32 {
        self.rigid_bodies.get(handle).map(|rb| rb.linvel().y).unwrap_or(0.0)
    }
}
