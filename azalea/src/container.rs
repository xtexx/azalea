use std::{fmt, fmt::Debug};

use azalea_client::{
    Client,
    inventory::{CloseContainerEvent, ContainerClickEvent, Inventory},
    packet::game::ReceiveGamePacketEvent,
};
use azalea_core::position::BlockPos;
use azalea_inventory::{
    ItemStack, Menu,
    operations::{ClickOperation, PickupClick, QuickMoveClick},
};
use azalea_physics::collision::BlockWithShape;
use azalea_protocol::packets::game::ClientboundGamePacket;
use bevy_app::{App, Plugin, Update};
use bevy_ecs::{component::Component, prelude::EventReader, system::Commands};
use futures_lite::Future;

use crate::bot::BotClientExt;

pub struct ContainerPlugin;
impl Plugin for ContainerPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Update, handle_menu_opened_event);
    }
}

pub trait ContainerClientExt {
    fn open_container_at(
        &self,
        pos: BlockPos,
    ) -> impl Future<Output = Option<ContainerHandle>> + Send;
    fn open_inventory(&self) -> Option<ContainerHandle>;
    fn get_held_item(&self) -> ItemStack;
    fn get_open_container(&self) -> Option<ContainerHandleRef>;
    fn view_container_or_inventory(&self) -> Menu;
}

impl ContainerClientExt for Client {
    /// Open a container in the world, like a chest. Use
    /// [`Client::open_inventory`] to open your own inventory.
    ///
    /// ```
    /// # use azalea::prelude::*;
    /// # async fn example(mut bot: azalea::Client) {
    /// let target_pos = bot
    ///     .world()
    ///     .read()
    ///     .find_block(bot.position(), &azalea::registry::Block::Chest.into());
    /// let Some(target_pos) = target_pos else {
    ///     bot.chat("no chest found");
    ///     return;
    /// };
    /// let container = bot.open_container_at(target_pos).await;
    /// # }
    /// ```
    async fn open_container_at(&self, pos: BlockPos) -> Option<ContainerHandle> {
        let mut ticks = self.get_tick_broadcaster();
        // wait until it's not air (up to 10 ticks)
        for _ in 0..10 {
            if !self
                .world()
                .read()
                .get_block_state(&pos)
                .unwrap_or_default()
                .is_collision_shape_empty()
            {
                break;
            }
            let _ = ticks.recv().await;
        }

        self.ecs
            .lock()
            .entity_mut(self.entity)
            .insert(WaitingForInventoryOpen);
        self.block_interact(pos);

        while ticks.recv().await.is_ok() {
            let ecs = self.ecs.lock();
            if ecs.get::<WaitingForInventoryOpen>(self.entity).is_none() {
                break;
            }
        }

        let ecs = self.ecs.lock();
        let inventory = ecs.get::<Inventory>(self.entity).expect("no inventory");
        if inventory.id == 0 {
            None
        } else {
            Some(ContainerHandle::new(inventory.id, self.clone()))
        }
    }

    /// Open the player's inventory. This will return None if another
    /// container is open.
    ///
    /// Note that this will send a packet to the server once it's dropped. Also,
    /// due to how it's implemented, you could call this function multiple times
    /// while another inventory handle already exists (but you shouldn't).
    ///
    /// If you just want to get the items in the player's inventory without
    /// sending any packets, use [`Client::menu`], [`Menu::player_slots_range`],
    /// and [`Menu::slots`].
    fn open_inventory(&self) -> Option<ContainerHandle> {
        let ecs = self.ecs.lock();
        let inventory = ecs.get::<Inventory>(self.entity).expect("no inventory");

        if inventory.id == 0 {
            Some(ContainerHandle::new(0, self.clone()))
        } else {
            None
        }
    }

    /// Get the item in the bot's hotbar that is currently being held in its
    /// main hand.
    fn get_held_item(&self) -> ItemStack {
        self.map_get_component::<Inventory, _>(|inventory| inventory.held_item())
            .expect("no inventory")
    }

    /// Get a handle to the open container. This will return None if no
    /// container is open. This will not close the container when it's dropped.
    ///
    /// See [`Client::open_inventory`] or [`Client::menu`] if you want to open
    /// your own inventory.
    fn get_open_container(&self) -> Option<ContainerHandleRef> {
        let ecs = self.ecs.lock();
        let inventory = ecs.get::<Inventory>(self.entity).expect("no inventory");

        if inventory.id == 0 {
            None
        } else {
            Some(ContainerHandleRef {
                id: inventory.id,
                client: self.clone(),
            })
        }
    }

    /// Returns the player's currently open container menu, or their inventory
    /// if no container is open.
    ///
    /// This tries to access the client's [`Inventory::container_menu`] and
    /// falls back to [`Inventory::inventory_menu`].
    fn view_container_or_inventory(&self) -> Menu {
        self.map_get_component::<Inventory, _>(|inventory| {
            inventory
                .container_menu
                .clone()
                .unwrap_or(inventory.inventory_menu.clone())
        })
        .expect("no inventory")
    }
}

/// A handle to a container that may be open. This does not close the container
/// when it's dropped. See [`ContainerHandle`] if that behavior is desired.
pub struct ContainerHandleRef {
    id: i32,
    client: Client,
}
impl Debug for ContainerHandleRef {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContainerHandle")
            .field("id", &self.id())
            .finish()
    }
}
impl ContainerHandleRef {
    pub fn close(&self) {
        self.client.ecs.lock().send_event(CloseContainerEvent {
            entity: self.client.entity,
            id: self.id,
        });
    }

    /// Get the id of the container. If this is 0, that means it's the player's
    /// inventory. Otherwise, the number isn't really meaningful since only one
    /// container can be open at a time.
    pub fn id(&self) -> i32 {
        self.id
    }

    /// Returns the menu of the container. If the container is closed, this
    /// will return `None`.
    ///
    /// Note that any modifications you make to the `Menu` you're given will not
    /// actually cause any packets to be sent. If you're trying to modify your
    /// inventory, use [`ContainerHandle::click`] instead
    pub fn menu(&self) -> Option<Menu> {
        let ecs = self.client.ecs.lock();
        let inventory = ecs
            .get::<Inventory>(self.client.entity)
            .expect("no inventory");

        // this also makes sure we can't access the inventory while a container is open
        if inventory.id == self.id {
            if self.id == 0 {
                Some(inventory.inventory_menu.clone())
            } else {
                Some(inventory.container_menu.clone().unwrap())
            }
        } else {
            None
        }
    }

    /// Returns the item slots in the container, not including the player's
    /// inventory. If the container is closed, this will return `None`.
    pub fn contents(&self) -> Option<Vec<ItemStack>> {
        self.menu().map(|menu| menu.contents())
    }

    /// Return the contents of the menu, including the player's inventory. If
    /// the container is closed, this will return `None`.
    pub fn slots(&self) -> Option<Vec<ItemStack>> {
        self.menu().map(|menu| menu.slots())
    }

    pub fn click(&self, operation: impl Into<ClickOperation>) {
        let operation = operation.into();
        self.client.ecs.lock().send_event(ContainerClickEvent {
            entity: self.client.entity,
            window_id: self.id,
            operation,
        });
    }
}

/// A handle to the open container. The container will be closed once this is
/// dropped.
pub struct ContainerHandle(ContainerHandleRef);

impl Drop for ContainerHandle {
    fn drop(&mut self) {
        self.0.close();
    }
}
impl Debug for ContainerHandle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("ContainerHandle")
            .field("id", &self.id())
            .finish()
    }
}
impl ContainerHandle {
    fn new(id: i32, client: Client) -> Self {
        Self(ContainerHandleRef { id, client })
    }

    /// Get the id of the container. If this is 0, that means it's the player's
    /// inventory. Otherwise, the number isn't really meaningful since only one
    /// container can be open at a time.
    pub fn id(&self) -> i32 {
        self.0.id()
    }

    /// Returns the menu of the container. If the container is closed, this
    /// will return `None`.
    ///
    /// Note that any modifications you make to the `Menu` you're given will not
    /// actually cause any packets to be sent. If you're trying to modify your
    /// inventory, use [`ContainerHandle::click`] instead
    pub fn menu(&self) -> Option<Menu> {
        self.0.menu()
    }

    /// Returns the item slots in the container, not including the player's
    /// inventory. If the container is closed, this will return `None`.
    pub fn contents(&self) -> Option<Vec<ItemStack>> {
        self.0.contents()
    }

    /// Return the contents of the menu, including the player's inventory. If
    /// the container is closed, this will return `None`.
    pub fn slots(&self) -> Option<Vec<ItemStack>> {
        self.0.slots()
    }

    /// Closes the inventory by dropping the handle.
    pub fn close(self) {
        // implicitly calls drop
    }

    pub fn click(&self, operation: impl Into<ClickOperation>) {
        self.0.click(operation);
    }

    /// A shortcut for [`Self::click`] with `PickupClick::Left`.
    pub fn left_click(&self, slot: impl Into<usize>) {
        self.click(PickupClick::Left {
            slot: Some(slot.into() as u16),
        });
    }
    /// A shortcut for [`Self::click`] with `QuickMoveClick::Left`.
    pub fn shift_click(&self, slot: impl Into<usize>) {
        self.click(QuickMoveClick::Left {
            slot: slot.into() as u16,
        });
    }
    /// A shortcut for [`Self::click`] with `PickupClick::Right`.
    pub fn right_click(&self, slot: impl Into<usize>) {
        self.click(PickupClick::Right {
            slot: Some(slot.into() as u16),
        });
    }
}

#[derive(Component, Debug)]
pub struct WaitingForInventoryOpen;

fn handle_menu_opened_event(
    mut commands: Commands,
    mut events: EventReader<ReceiveGamePacketEvent>,
) {
    for event in events.read() {
        if let ClientboundGamePacket::ContainerSetContent { .. } = event.packet.as_ref() {
            commands
                .entity(event.entity)
                .remove::<WaitingForInventoryOpen>();
        }
    }
}
