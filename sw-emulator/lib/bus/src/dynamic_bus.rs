/*++

Licensed under the Apache-2.0 license.

File Name:

    dynamic_bus.rs

Abstract:

    File contains DynamicBus type.

--*/

use std::{io::ErrorKind, ops::RangeInclusive};

use crate::Bus;
use caliptra_emu_types::{RvAddr, RvData, RvException, RvSize};

struct MappedDevice {
    name: String,
    mmap_range: RangeInclusive<RvAddr>,
    bus: Box<dyn Bus>,
}

/// A bus that uses dynamic-dispatch to delegate to a runtime-modifiable list of
/// devices. Useful as a quick-and-dirty Bus implementation.
pub struct DynamicBus {
    /// Devices connected to the CPU
    devs: Vec<MappedDevice>,
}

impl DynamicBus {
    pub fn new() -> DynamicBus {
        Self { devs: Vec::new() }
    }
    /// Attach the specified device to the CPU
    ///
    /// # Arguments
    ///
    /// * `dev` - Device to attach
    pub fn attach_dev(
        &mut self,
        name: &str,
        mmap_range: RangeInclusive<RvAddr>,
        bus: Box<dyn Bus>,
    ) -> std::io::Result<()> {
        let dev = MappedDevice {
            name: name.into(),
            mmap_range,
            bus: bus,
        };
        let dev_addr = dev.mmap_range.clone();
        let mut index = 0;
        for cur_dev in self.devs.iter() {
            let cur_dev_addr = cur_dev.mmap_range.clone();
            // Check if the device range overlaps existing device
            if dev_addr.end() >= cur_dev_addr.start() && dev_addr.start() <= cur_dev_addr.end() {
                return Err(std::io::Error::new(
                    ErrorKind::AddrInUse,
                    format!("Address space for device {} ({:#010x}-{:#010x}) collides with device {} ({:#010x}-{:#010x})",
                    dev.name, dev.mmap_range.start(), dev.mmap_range.end(),
                    cur_dev.name, cur_dev.mmap_range.start(), cur_dev.mmap_range.end())));
            }
            // Found the position to insert the device
            if dev_addr.start() < cur_dev_addr.start() {
                break;
            }
            index += 1;
        }
        self.devs.insert(index, dev);
        Ok(())
    }
}

impl Bus for DynamicBus {
    fn read(&self, size: RvSize, addr: RvAddr) -> Result<RvData, RvException> {
        let dev = self.devs.iter().find(|d| d.mmap_range.contains(&addr));
        match dev {
            Some(dev) => dev.bus.read(size, addr - dev.mmap_range.start()),
            None => Err(RvException::load_access_fault(addr)),
        }
    }

    fn write(&mut self, size: RvSize, addr: RvAddr, val: RvData) -> Result<(), RvException> {
        let dev = self.devs.iter_mut().find(|d| d.mmap_range.contains(&addr));
        match dev {
            Some(dev) => dev.bus.write(size, addr - dev.mmap_range.start(), val),
            None => Err(RvException::store_access_fault(addr)),
        }
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{Ram, Rom};
    use caliptra_emu_types::RvSize;

    #[test]
    fn test_dynamic_bus_read() {
        let mut bus = DynamicBus::new();
        let rom = Rom::new(vec![1, 2]);
        bus.attach_dev("ROM0", 1..=2, Box::new(rom)).unwrap();
        assert_eq!(bus.read(RvSize::Byte, 1).ok(), Some(1));
        assert_eq!(bus.read(RvSize::Byte, 2).ok(), Some(2));
        assert_eq!(
            bus.read(RvSize::Byte, 3).err(),
            Some(RvException::load_access_fault(3))
        );
    }

    #[test]
    fn test_dynamic_bus_write() {
        let mut bus = DynamicBus::new();
        let rom = Ram::new(vec![1, 2]);
        bus.attach_dev("RAM0", 1..=2, Box::new(rom)).unwrap();
        assert_eq!(bus.write(RvSize::Byte, 1, 3).ok(), Some(()));
        assert_eq!(bus.read(RvSize::Byte, 1).ok(), Some(3));
        assert_eq!(bus.write(RvSize::Byte, 2, 4).ok(), Some(()));
        assert_eq!(bus.read(RvSize::Byte, 2).ok(), Some(4));
        assert_eq!(
            bus.write(RvSize::Byte, 3, 0).err(),
            Some(RvException::store_access_fault(3))
        );
    }

    fn is_sorted<T>(slice: &[T]) -> bool
    where
        T: Ord,
    {
        slice.windows(2).all(|s| s[0] <= s[1])
    }

    #[test]
    fn test_attach_dev() {
        let mut bus = DynamicBus::new();
        let rom = Rom::new(vec![1, 2]);
        // Attach valid devices
        bus.attach_dev("ROM0", 1..=2, Box::new(rom)).unwrap();
        let rom = Rom::new(vec![1]);
        bus.attach_dev("ROM1", 0..=0, Box::new(rom)).unwrap();
        let rom = Rom::new(vec![1]);
        bus.attach_dev("ROM2", 3..=3, Box::new(rom)).unwrap();

        // Try inserting devices whose address maps overlap existing devices

        let rom = Rom::new(vec![1]);
        let err = bus.attach_dev("ROM3", 1..=1, Box::new(rom)).err().unwrap();
        assert_eq!(err.to_string(), "Address space for device ROM3 (0x00000001-0x00000001) collides with device ROM0 (0x00000001-0x00000002)");

        let rom = Rom::new(vec![1]);
        let err = bus.attach_dev("ROM4", 2..=2, Box::new(rom)).err().unwrap();
        assert_eq!(err.to_string(), "Address space for device ROM4 (0x00000002-0x00000002) collides with device ROM0 (0x00000001-0x00000002)");

        let addrs: Vec<RvAddr> = bus
            .devs
            .iter()
            .flat_map(|d| [*d.mmap_range.start(), *d.mmap_range.end()])
            .collect();
        assert_eq!(addrs.len(), 6);
        assert!(is_sorted(&addrs));
    }
}
