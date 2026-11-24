//! Object-safe cloning of the boxed state a switch case carries.

use std::any::Any;

use super::SwitchCaseData;

/// Object-safe `Clone` for the boxed state; the blanket impl reaches `Clone`
/// through the concrete type, since a `Box<dyn SwitchStruct>` is not `Clone`.
pub trait SwitchClone {
    fn switch_clone(&self) -> Box<dyn SwitchStruct>;
}

pub trait SwitchStruct: SwitchClone + 'static {
    fn as_any(&self) -> &dyn Any;
}

impl<T: SwitchStruct + Clone> SwitchClone for T {
    fn switch_clone(&self) -> Box<dyn SwitchStruct> {
        Box::new(self.to_owned())
    }
}

impl Clone for SwitchCaseData {
    fn clone(&self) -> Self {
        match *self {
            SwitchCaseData::None => Self::None,
            SwitchCaseData::Bool(value) => Self::Bool(value),
            SwitchCaseData::Char(value) => Self::Char(value),
            SwitchCaseData::Struct(ref _struct) => Self::Struct(_struct.switch_clone()),
        }
    }
}
