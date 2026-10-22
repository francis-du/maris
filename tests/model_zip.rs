use candle_core::npy::NpzTensors;
use candle_core::{Device, Error, Result, Tensor};
use std::path::Path;

fn write_tensor(path: &Path, name: &str) -> Result<()> {
    let tensor = Tensor::new(&[2u32, 4, 6], &Device::Cpu)?;
    Tensor::write_npz(&[(name, &tensor)], path)?;
    Ok(())
}

fn assert_tensor(path: &Path, name: &str) -> Result<()> {
    let loaded = Tensor::read_npz(path)?;
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].0, name);
    assert_eq!(loaded[0].1.to_vec1::<u32>()?, vec![2, 4, 6]);
    let lazy = NpzTensors::new(path)?;
    assert_eq!(
        lazy.names()
            .iter()
            .map(|value| value.as_str())
            .collect::<Vec<_>>(),
        vec![name]
    );
    assert_eq!(
        lazy.get(name)?
            .expect("named tensor is indexed")
            .to_vec1::<u32>()?,
        vec![2, 4, 6]
    );
    Ok(())
}

#[test]
fn zip9_npz_preserves_utf8_names_and_tensor_values() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("utf8.npz");
    write_tensor(&path, "donn\u{e9}es")?;
    assert_tensor(&path, "donn\u{e9}es")
}

#[test]
fn zip9_npz_preserves_owned_cp437_names_and_tensor_values() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("cp437.npz");
    write_tensor(&path, "a")?;
    let mut bytes = std::fs::read(&path)?;
    let positions: Vec<_> = bytes
        .windows(5)
        .enumerate()
        .filter_map(|(index, value)| (value == b"a.npy").then_some(index))
        .collect();
    assert_eq!(positions.len(), 2);
    for index in positions {
        bytes[index] = 0x82;
    }
    std::fs::write(&path, bytes)?;
    assert_tensor(&path, "\u{e9}")
}

#[test]
fn invalid_npz_is_reported_as_a_zip_error() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("invalid.npz");
    std::fs::write(&path, b"not a zip archive")?;
    assert!(matches!(Tensor::read_npz(&path), Err(Error::Zip(_))));
    assert!(matches!(NpzTensors::new(&path), Err(Error::Zip(_))));
    Ok(())
}

#[test]
fn invalid_pth_is_reported_as_a_zip_error() -> Result<()> {
    let directory = tempfile::tempdir()?;
    let path = directory.path().join("invalid.pth");
    std::fs::write(&path, b"not a zip archive")?;
    assert!(matches!(
        candle_core::pickle::read_pth_tensor_info(&path, false, None),
        Err(Error::Zip(_))
    ));
    Ok(())
}
