pub mod node;
pub mod animation;

use crate::{
    utils::{
        UnsafeCollectionView,
        visitor::{
            Visit,
            VisitResult,
            Visitor,
        },
        pool::{
            Handle,
            Pool,
        },
    },
    math::mat4::Mat4,
    physics::Physics,
    engine::State,
    scene::{
        animation::Animation,
        node::Node,
        node::NodeKind,
    },
};
use crate::math::vec3::Vec3;
use std::collections::HashMap;

pub struct Scene {
    /// Nodes pool, every node lies inside pool. User-code may borrow
    /// a reference to a node using handle.
    nodes: Pool<Node>,
    /// Root node of scene. Each node added to scene will be attached
    /// to root.
    root: Handle<Node>,
    animations: Pool<Animation>,
    physics: Physics,
    /// Tree traversal stack.
    stack: Vec<Handle<Node>>,
    active_camera: Handle<Node>
}

impl Default for Scene {
    fn default() -> Self {
        Self {
            nodes: Pool::new(),
            root: Default::default(),
            physics: Physics::new(),
            stack: Vec::new(),
            animations: Pool::new(),
            active_camera: Handle::none(),
        }
    }
}

impl Scene {
    #[inline]
    pub fn new() -> Scene {
        let mut nodes: Pool<Node> = Pool::new();
        let root = nodes.spawn(Node::new(NodeKind::Base));
        Scene {
            nodes,
            stack: Vec::new(),
            root,
            animations: Pool::new(),
            physics: Physics::new(),
            active_camera: Handle::none(),
        }
    }

    /// Transfers ownership of node into scene.
    /// Returns handle to node.
    #[inline]
    pub fn add_node(&mut self, node: Node) -> Handle<Node> {
        let handle = self.nodes.spawn(node);
        self.link_nodes(handle, self.root);
        if let Some(node) = self.nodes.borrow(handle) {
            if let NodeKind::Camera(_) = node.borrow_kind() {
                self.active_camera = handle;
            }
        }
        handle
    }

    #[inline]
    pub fn get_nodes(&mut self) -> &Pool<Node> {
        &self.nodes
    }

    #[inline]
    pub fn get_nodes_mut(&mut self) -> &mut Pool<Node> {
        &mut self.nodes
    }

    pub fn get_active_camera(&self) -> Option<&Node> {
        self.nodes.borrow(self.active_camera)
    }

    /// Destroys node and its children recursively.
    #[inline]
    pub fn remove_node(&mut self, node_handle: Handle<Node>, state: &mut State) {
        let mut children = UnsafeCollectionView::empty();

        if let Some(node) = self.nodes.borrow_mut(node_handle) {
            self.physics.remove_body(node.get_body());
            children = UnsafeCollectionView::from_slice(&node.children);
        }

        // Free children recursively
        for child_handle in children.iter() {
            self.remove_node(child_handle.clone(), state);
        }

        self.nodes.free(node_handle);
    }

    #[inline]
    pub fn get_node(&self, handle: Handle<Node>) -> Option<&Node> {
        self.nodes.borrow(handle)
    }

    #[inline]
    pub fn get_node_mut(&mut self, handle: Handle<Node>) -> Option<&mut Node> {
        self.nodes.borrow_mut(handle)
    }

    #[inline]
    pub fn get_physics(&self) -> &Physics {
        &self.physics
    }

    #[inline]
    pub fn get_physics_mut(&mut self) -> &mut Physics {
        &mut self.physics
    }

    #[inline]
    pub fn add_animation(&mut self, animation: Animation) -> Handle<Animation> {
        self.animations.spawn(animation)
    }

    #[inline]
    pub fn get_animation(&self, handle: Handle<Animation>) -> Option<&Animation> {
        self.animations.borrow(handle)
    }

    #[inline]
    pub fn get_animation_mut(&mut self, handle: Handle<Animation>) -> Option<&mut Animation> {
        self.animations.borrow_mut(handle)
    }

    #[inline]
    pub fn get_animations(&self) -> &Pool<Animation> {
        &self.animations
    }

    /// Links specified child with specified parent.
    #[inline]
    pub fn link_nodes(&mut self, child_handle: Handle<Node>, parent_handle: Handle<Node>) {
        self.unlink_node(child_handle);
        if let Some(child) = self.nodes.borrow_mut(child_handle) {
            child.parent = parent_handle;
            if let Some(parent) = self.nodes.borrow_mut(parent_handle) {
                parent.children.push(child_handle.clone());
            }
        }
    }

    /// Unlinks specified node from its parent, so node will become root.
    #[inline]
    pub fn unlink_node(&mut self, node_handle: Handle<Node>) {
        let mut parent_handle: Handle<Node> = Handle::none();
        // Replace parent handle of child
        if let Some(node) = self.nodes.borrow_mut(node_handle) {
            parent_handle = node.parent;
            node.parent = Handle::none();
        }
        // Remove child from parent's children list
        if let Some(parent) = self.nodes.borrow_mut(parent_handle) {
            if let Some(i) = parent.children.iter().position(|h| *h == node_handle) {
                parent.children.remove(i);
            }
        }
    }

    /// Searches node with specified name starting from specified root node.
    pub fn find_node_by_name(&self, root_node: Handle<Node>, name: &str) -> Handle<Node> {
        match self.nodes.borrow(root_node) {
            Some(node) => {
                if node.get_name() == name {
                    root_node
                } else {
                    let mut result: Handle<Node> = Handle::none();
                    for child in &node.children {
                        let child_handle = self.find_node_by_name(*child, name);
                        if !child_handle.is_none() {
                            result = child_handle;
                            break;
                        }
                    }
                    result
                }
            }
            None => Handle::none()
        }
    }

    /// Creates a full copy of node with all children.
    /// This is relatively heavy operation!
    /// In case if some error happened it returns Handle::none
    pub fn copy_node(&self, root_handle: Handle<Node>, dest_scene: &mut Scene, old_new_mapping: &mut HashMap<Handle<Node>, Handle<Node>>) -> Handle<Node> {
        match self.get_node(root_handle) {
            Some(src_node) => {
                let mut dest_node = src_node.make_copy(root_handle.clone());
                if let Some(src_body) = self.physics.borrow_body(src_node.get_body()) {
                    dest_node.set_body(dest_scene.physics.add_body(src_body.make_copy()));
                }
                let dest_copy_handle = dest_scene.add_node(dest_node);
                old_new_mapping.insert(root_handle.clone(), dest_copy_handle);
                for src_child_handle in &src_node.children {
                    let dest_child_handle = self.copy_node(*src_child_handle, dest_scene, old_new_mapping);
                    if !dest_child_handle.is_none() {
                        dest_scene.link_nodes(dest_child_handle, dest_copy_handle);
                    }
                }
                dest_copy_handle
            }
            None => Handle::none()
        }
    }

    #[inline]
    pub fn get_root(&self) -> Handle<Node> {
        self.root
    }

    pub fn update_physics(&mut self, dt: f64) {
        self.physics.step(dt as f32);

        // Sync node positions with assigned physics bodies
        for node in self.nodes.iter_mut() {
            if let Some(body) = self.physics.borrow_body(node.get_body()) {
                node.set_local_position(body.get_position());
            }
        }
    }

    pub fn update_nodes(&mut self) {
        // Calculate transforms on nodes
        self.stack.clear();
        self.stack.push(self.root);
        while let Some(handle) = self.stack.pop() {
            // Calculate local transform and get parent handle
            let mut parent_handle: Handle<Node> = Handle::none();
            if let Some(node) = self.nodes.borrow_mut(handle) {
                node.calculate_local_transform();
                parent_handle = node.parent;
            }

            // Extract parent's global transform
            let mut parent_global_transform = Mat4::identity();
            let mut parent_visibility = true;
            if let Some(parent) = self.nodes.borrow(parent_handle) {
                parent_global_transform = parent.global_transform;
                parent_visibility = parent.global_visibility;
            }

            if let Some(node) = self.nodes.borrow_mut(handle) {
                node.global_transform = parent_global_transform * node.local_transform;
                node.global_visibility = parent_visibility && node.visibility;

                // Queue children and continue traversal on them
                for child_handle in node.children.iter() {
                    self.stack.push(child_handle.clone());
                }
            }
        }
    }

    pub fn update_animations(&mut self, dt: f32) {
        // Reset local transform of animated nodes first
        for animation in self.animations.iter() {
            for track in animation.get_tracks() {
                if let Some(node) = self.nodes.borrow_mut(track.get_node()) {
                    node.set_local_position(Default::default());
                    node.set_local_rotation(Default::default());
                    node.set_local_scale(Vec3::make(1.0, 1.0, 1.0));
                }
            }
        }

        // Then apply animation.
        for animation in self.animations.iter_mut() {
            let next_time_pos = animation.get_time_position() + dt * animation.get_speed();

            for track in animation.get_tracks() {
                if let Some(keyframe) = track.get_key_frame(animation.get_time_position()) {
                    if let Some(node) = self.nodes.borrow_mut(track.get_node()) {
                        node.set_local_position(node.get_local_position() + keyframe.position);
                        node.set_local_rotation(node.get_local_rotation() * keyframe.rotation);
                        node.set_local_scale(node.get_local_scale() * keyframe.scale);
                    }
                }
            }

            animation.set_time_position(next_time_pos);
            animation.update_fading(dt);
        }
    }

    pub fn update(&mut self, aspect_ratio: f32, dt: f64) {
        self.update_physics(dt);

        self.update_animations(dt as f32);
        self.update_nodes();

        for node in self.nodes.iter_mut() {
            let eye = node.get_global_position();
            let look = node.get_look_vector();
            let up = node.get_up_vector();
            if let NodeKind::Camera(camera) = node.borrow_kind_mut() {
                camera.calculate_matrices(eye, look, up, aspect_ratio);
            }
        }
    }
}

impl Visit for Scene {
    fn visit(&mut self, name: &str, visitor: &mut Visitor) -> VisitResult {
        visitor.enter_region(name)?;
        self.root.visit("Root", visitor)?;
        self.active_camera.visit("ActiveCamera", visitor)?;
        self.nodes.visit("Nodes", visitor)?;
        self.animations.visit("Animations", visitor)?;
        self.physics.visit("Physics", visitor)?;
        visitor.leave_region()
    }
}
