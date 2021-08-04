use super::{TypeInfo, WorkloadSystem};
use crate::all_storages::AllStorages;
use crate::borrow::{Borrow, BorrowInfo, Mutability};
use crate::error;
use crate::storage::StorageId;
use crate::type_id::TypeId;
use crate::World;
use alloc::boxed::Box;
use alloc::vec::Vec;
use core::any::type_name;
#[cfg(not(feature = "std"))]
use core::any::Any;
#[cfg(feature = "std")]
use std::error::Error;

/// Trait used to add systems to a workload.
///
/// Usually you don't have to use it directly.
pub trait IntoWorkloadSystem<B, R> {
    /// Wraps a function in a struct containing all information required by a workload.
    fn into_workload_system(self) -> Result<WorkloadSystem, error::InvalidSystem>;
    /// Wraps a failible function in a struct containing all information required by a workload.  
    /// The workload will stop if an error is returned.
    #[cfg(feature = "std")]
    fn into_workload_try_system<Ok, Err: Into<Box<dyn Error + Send + Sync>>>(
        self,
    ) -> Result<WorkloadSystem, error::InvalidSystem>
    where
        R: Into<Result<Ok, Err>>;
    /// Wraps a failible function in a struct containing all information required by a workload.  
    /// The workload will stop if an error is returned.
    #[cfg(not(feature = "std"))]
    fn into_workload_try_system<Ok, Err: 'static + Send + Any>(
        self,
    ) -> Result<WorkloadSystem, error::InvalidSystem>
    where
        R: Into<Result<Ok, Err>>;
}

pub struct Untracked;

impl<R, F> IntoWorkloadSystem<Untracked, R> for F
where
    F: 'static + Send + Sync + Fn() -> R,
{
    fn into_workload_system(self) -> Result<WorkloadSystem, error::InvalidSystem> {
        Ok(WorkloadSystem {
            borrow_constraints: Vec::new(),
            system_fn: Box::new(move |_: &World| {
                (self)();
                Ok(())
            }),
            system_type_id: TypeId::of::<F>(),
            system_type_name: type_name::<F>(),
            generator: |_| TypeId::of::<F>(),
        })
    }
    #[cfg(feature = "std")]
    fn into_workload_try_system<Ok, Err: Into<Box<dyn Error + Send + Sync>>>(
        self,
    ) -> Result<WorkloadSystem, error::InvalidSystem>
    where
        R: Into<Result<Ok, Err>>,
    {
        Ok(WorkloadSystem {
            borrow_constraints: Vec::new(),
            system_fn: Box::new(move |_: &World| {
                (self)().into().map_err(error::Run::from_custom)?;
                Ok(())
            }),
            system_type_id: TypeId::of::<F>(),
            system_type_name: type_name::<F>(),
            generator: |_| TypeId::of::<F>(),
        })
    }
    #[cfg(not(feature = "std"))]
    fn into_workload_try_system<Ok, Err: 'static + Send + Any>(
        self,
    ) -> Result<WorkloadSystem, error::InvalidSystem>
    where
        R: Into<Result<Ok, Err>>,
    {
        Ok(WorkloadSystem {
            borrow_constraints: Vec::new(),
            system_fn: Box::new(move |_: &World| {
                (self)().into().map_err(error::Run::from_custom)?;
                Ok(())
            }),
            system_type_id: TypeId::of::<F>(),
            system_type_name: type_name::<F>(),
            generator: |_| TypeId::of::<F>(),
        })
    }
}

impl IntoWorkloadSystem<(), ()> for WorkloadSystem {
    fn into_workload_system(self) -> Result<WorkloadSystem, error::InvalidSystem> {
        Ok(self)
    }
    #[cfg(feature = "std")]
    fn into_workload_try_system<Ok, Err>(self) -> Result<WorkloadSystem, error::InvalidSystem> {
        Ok(self)
    }
    #[cfg(not(feature = "std"))]
    fn into_workload_try_system<Ok, Err>(self) -> Result<WorkloadSystem, error::InvalidSystem> {
        Ok(self)
    }
}

macro_rules! impl_system {
    ($(($type: ident, $index: tt))+) => {
        impl<$($type: Borrow + BorrowInfo,)+ R, Func> IntoWorkloadSystem<($($type,)+), R> for Func
        where
            Func: 'static
                + Send
                + Sync,
            for<'a, 'b> &'b Func:
                Fn($($type),+) -> R
                + Fn($($type::View<'_>),+) -> R {

            fn into_workload_system(self) -> Result<WorkloadSystem, error::InvalidSystem> {
                let mut borrows = Vec::new();
                $(
                    $type::borrow_info(&mut borrows);
                )+

                if borrows.contains(&TypeInfo {
                    name: "",
                    storage_id: StorageId::of::<AllStorages>(),
                    mutability: Mutability::Exclusive,
                    is_send: true,
                    is_sync: true,
                }) && borrows.len() > 1
                {
                    return Err(error::InvalidSystem::AllStorages);
                }

                let mid = borrows.len() / 2 + (borrows.len() % 2 != 0) as usize;

                for a_type_info in &borrows[..mid] {
                    for b_type_info in &borrows[mid..] {
                        if a_type_info.storage_id == b_type_info.storage_id {
                            match (a_type_info.mutability, b_type_info.mutability) {
                                (Mutability::Exclusive, Mutability::Exclusive) => {
                                    return Err(error::InvalidSystem::MultipleViewsMut)
                                }
                                (Mutability::Exclusive, Mutability::Shared)
                                | (Mutability::Shared, Mutability::Exclusive) => {
                                    return Err(error::InvalidSystem::MultipleViews)
                                }
                                (Mutability::Shared, Mutability::Shared) => {}
                            }
                        }
                    }
                }

                Ok(WorkloadSystem {
                    borrow_constraints: borrows,
                    system_fn: Box::new(move |world: &World| { Ok(drop((&&self)($($type::borrow(&world)?),+))) }),
                    system_type_id: TypeId::of::<Func>(),
                    system_type_name: type_name::<Func>(),
                    generator: |constraints| {
                        $(
                            $type::borrow_info(constraints);
                        )+

                        TypeId::of::<Func>()
                    },
                })
            }
            #[cfg(feature = "std")]
            fn into_workload_try_system<Ok, Err: Into<Box<dyn Error + Send + Sync>>>(self) -> Result<WorkloadSystem, error::InvalidSystem> where R: Into<Result<Ok, Err>> {
                let mut borrows = Vec::new();
                $(
                    $type::borrow_info(&mut borrows);
                )+

                if borrows.contains(&TypeInfo {
                    name: "",
                    storage_id: StorageId::of::<AllStorages>(),
                    mutability: Mutability::Exclusive,
                    is_send: true,
                    is_sync: true,
                }) && borrows.len() > 1
                {
                    return Err(error::InvalidSystem::AllStorages);
                }

                let mid = borrows.len() / 2 + (borrows.len() % 2 != 0) as usize;

                for a_type_info in &borrows[..mid] {
                    for b_type_info in &borrows[mid..] {
                        if a_type_info.storage_id == b_type_info.storage_id {
                            match (a_type_info.mutability, b_type_info.mutability) {
                                (Mutability::Exclusive, Mutability::Exclusive) => {
                                    return Err(error::InvalidSystem::MultipleViewsMut)
                                }
                                (Mutability::Exclusive, Mutability::Shared)
                                | (Mutability::Shared, Mutability::Exclusive) => {
                                    return Err(error::InvalidSystem::MultipleViews)
                                }
                                (Mutability::Shared, Mutability::Shared) => {}
                            }
                        }
                    }
                }

                Ok(WorkloadSystem {
                    borrow_constraints: borrows,
                    system_fn: Box::new(move |world: &World| { Ok(drop((&&self)($($type::borrow(&world)?),+).into().map_err(error::Run::from_custom)?)) }),
                    system_type_id: TypeId::of::<Func>(),
                    system_type_name: type_name::<Func>(),
                    generator: |constraints| {
                        $(
                            $type::borrow_info(constraints);
                        )+

                        TypeId::of::<Func>()
                    },
                })
            }
            #[cfg(not(feature = "std"))]
            fn into_workload_try_system<Ok, Err: 'static + Send + Any>(self) -> Result<WorkloadSystem, error::InvalidSystem> where R: Into<Result<Ok, Err>> {
                let mut borrows = Vec::new();
                $(
                    $type::borrow_info(&mut borrows);
                )+

                if borrows.contains(&TypeInfo {
                    name: "",
                    storage_id: StorageId::of::<AllStorages>(),
                    mutability: Mutability::Exclusive,
                    is_send: true,
                    is_sync: true,
                }) && borrows.len() > 1
                {
                    return Err(error::InvalidSystem::AllStorages);
                }

                let mid = borrows.len() / 2 + (borrows.len() % 2 != 0) as usize;

                for a_type_info in &borrows[..mid] {
                    for b_type_info in &borrows[mid..] {
                        if a_type_info.storage_id == b_type_info.storage_id {
                            match (a_type_info.mutability, b_type_info.mutability) {
                                (Mutability::Exclusive, Mutability::Exclusive) => {
                                    return Err(error::InvalidSystem::MultipleViewsMut)
                                }
                                (Mutability::Exclusive, Mutability::Shared)
                                | (Mutability::Shared, Mutability::Exclusive) => {
                                    return Err(error::InvalidSystem::MultipleViews)
                                }
                                (Mutability::Shared, Mutability::Shared) => {}
                            }
                        }
                    }
                }

                Ok(WorkloadSystem {
                    borrow_constraints: borrows,
                    system_fn: Box::new(move |world: &World| { Ok(drop((&&self)($($type::borrow(&world)?),+).into().map_err(error::Run::from_custom)?)) }),
                    system_type_id: TypeId::of::<Func>(),
                    system_type_name: type_name::<Func>(),
                    generator: |constraints| {
                        $(
                            $type::borrow_info(constraints);
                        )+

                        TypeId::of::<Func>()
                    },
                })
            }
        }
    }
}

macro_rules! system {
    ($(($type: ident, $index: tt))*;($type1: ident, $index1: tt) $(($queue_type: ident, $queue_index: tt))*) => {
        impl_system![$(($type, $index))*];
        system![$(($type, $index))* ($type1, $index1); $(($queue_type, $queue_index))*];
    };
    ($(($type: ident, $index: tt))*;) => {
        impl_system![$(($type, $index))*];
    }
}

system![(A, 0); (B, 1) (C, 2) (D, 3) (E, 4) (F, 5) (G, 6) (H, 7) (I, 8) (J, 9)];
