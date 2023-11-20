//! Adds important functionalities
use sysfs_gpio::Pin;

/// Represents a single device of any type; GPIO-based, SPI-based, I2C-based,
/// etc.  It defines the basic services applicable to all devices
pub trait Device {
    /// Get the pin
    fn pin(&self) -> Pin;

    /// Shut down the device and release all associated resources.
    fn close(&self) {
        let pin = self.pin();
        if pin.is_exported() {
            //TODO implement better error handling
            pin.unexport().expect("Could not close device");
        }
    }

    /// Returns a value representing the device's state.
    fn value(&self) -> i8;

    /// Returns ``True`` if the device is currently active and ``False``otherwise.
    fn is_active(&self) -> bool {
        let value = self.value();
        value >= 1
    }
}

/// Adds edge-detected `when_activated` and `when_deactivated`
/// events to a device based on changes to the `is_active`
/// property common to all devices.
pub trait EventsTrait: Device {
    /// Pause the program until the device is activated
    fn wait_for_active(&self) {
        loop {
            if self.is_active() {
                break;
            }
        }
    }

    /// Pause the program until the device is deactivated
    fn wait_for_inactive(&self) {
        loop {
            if !self.is_active() {
                break;
            }
        }
    }
}

/// Represents a device composed of multiple devices like simple HATs,
/// H-bridge motor controllers, robots composed of multiple motors, etc.
pub trait CompositeDevices {
    /// Shut down the device and release all associated resources.
    fn close(&self);
}
