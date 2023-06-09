use std::{
    io,
    marker::PhantomData,
    ops::{Deref, DerefMut},
};

use clear_on_drop::clear::Clear;
use rand::{rngs::OsRng, RngCore};
use serde::{Deserialize, Serialize};

use nimiq_database_value::{FromDatabaseValue, IntoDatabaseValue};
use nimiq_hash::argon2kdf::{compute_argon2_kdf, Argon2Error};

pub trait Verify {
    fn verify(&self) -> bool;
}

// Own ClearOnDrop
struct ClearOnDrop<T: Clear> {
    place: Option<T>,
}

impl<T: Clear> ClearOnDrop<T> {
    #[inline]
    fn new(place: T) -> Self {
        ClearOnDrop { place: Some(place) }
    }

    #[inline]
    fn into_uncleared_place(mut c: Self) -> T {
        // By invariance, c.place must be Some(...).
        c.place.take().unwrap()
    }
}

impl<T: Clear> Drop for ClearOnDrop<T> {
    #[inline]
    fn drop(&mut self) {
        // Make sure to drop the unlocked data.
        if let Some(ref mut data) = self.place {
            data.clear();
        }
    }
}

impl<T: Clear> Deref for ClearOnDrop<T> {
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        // By invariance, c.place must be Some(...).
        self.place.as_ref().unwrap()
    }
}

impl<T: Clear> DerefMut for ClearOnDrop<T> {
    #[inline]
    fn deref_mut(&mut self) -> &mut Self::Target {
        // By invariance, c.place must be Some(...).
        self.place.as_mut().unwrap()
    }
}

impl<T: Clear> AsRef<T> for ClearOnDrop<T> {
    #[inline]
    fn as_ref(&self) -> &T {
        // By invariance, c.place must be Some(...).
        self.place.as_ref().unwrap()
    }
}

// Unlocked container
pub struct Unlocked<T>
where
    for<'de> T: 'de + Clear + Deserialize<'de> + Serialize,
{
    data: ClearOnDrop<T>,
    lock: Locked<T>,
}

impl<T> Unlocked<T>
where
    for<'de> T: 'de + Clear + Deserialize<'de> + Serialize,
{
    /// Calling code should make sure to clear the password from memory after use.
    pub fn new(
        secret: T,
        password: &[u8],
        iterations: u32,
        salt_length: usize,
    ) -> Result<Self, Argon2Error> {
        let locked = Locked::create(&secret, password, iterations, salt_length)?;
        Ok(Unlocked {
            data: ClearOnDrop::new(secret),
            lock: locked,
        })
    }

    /// Calling code should make sure to clear the password from memory after use.
    pub fn with_defaults(secret: T, password: &[u8]) -> Result<Self, Argon2Error> {
        Self::new(
            secret,
            password,
            OtpLock::<T>::DEFAULT_ITERATIONS,
            OtpLock::<T>::DEFAULT_SALT_LENGTH,
        )
    }

    #[inline]
    pub fn lock(lock: Self) -> Locked<T> {
        // ClearOnDrop makes sure the unlocked data is not leaked.
        lock.lock
    }

    #[inline]
    pub fn into_otp_lock(lock: Self) -> OtpLock<T> {
        OtpLock::Unlocked(lock)
    }

    #[inline]
    pub fn into_unlocked_data(lock: Self) -> T {
        ClearOnDrop::into_uncleared_place(lock.data)
    }

    #[inline]
    pub fn unlocked_data(lock: &Self) -> &T {
        &lock.data
    }
}

impl<T> Deref for Unlocked<T>
where
    for<'de> T: 'de + Clear + Deserialize<'de> + Serialize,
{
    type Target = T;

    #[inline]
    fn deref(&self) -> &Self::Target {
        &self.data
    }
}

// Locked container
#[derive(Serialize, Deserialize)]
pub struct Locked<T>
where
    for<'a> T: 'a + Clear + Deserialize<'a> + Serialize,
{
    lock: Vec<u8>,
    salt: Vec<u8>,
    iterations: u32,
    #[serde(skip)]
    phantom: PhantomData<T>,
}

impl<T> Locked<T>
where
    for<'de> T: 'de + Clear + Deserialize<'de> + Serialize,
{
    /// Calling code should make sure to clear the password from memory after use.
    pub fn new(
        mut secret: T,
        password: &[u8],
        iterations: u32,
        salt_length: usize,
    ) -> Result<Self, Argon2Error> {
        let result = Locked::create(&secret, password, iterations, salt_length)?;

        // Remove secret from memory.
        secret.clear();

        Ok(result)
    }

    /// Calling code should make sure to clear the password from memory after use.
    pub fn with_defaults(secret: T, password: &[u8]) -> Result<Self, Argon2Error> {
        Self::new(
            secret,
            password,
            OtpLock::<T>::DEFAULT_ITERATIONS,
            OtpLock::<T>::DEFAULT_SALT_LENGTH,
        )
    }

    /// Calling code should make sure to clear the password from memory after use.
    /// The integrity of the output value is not checked.
    pub fn unlock_unchecked(self, password: &[u8]) -> Result<Unlocked<T>, Locked<T>> {
        let key_opt = Self::otp(&self.lock, password, self.iterations, &self.salt).ok();
        let mut key = if let Some(key_content) = key_opt {
            key_content
        } else {
            return Err(self);
        };

        let result = postcard::from_bytes(&key).ok();

        // Always overwrite unencrypted vector.
        for byte in key.iter_mut() {
            byte.clear();
        }

        if let Some(data) = result {
            Ok(Unlocked {
                data: ClearOnDrop::new(data),
                lock: self,
            })
        } else {
            Err(self)
        }
    }

    fn otp(
        secret: &[u8],
        password: &[u8],
        iterations: u32,
        salt: &[u8],
    ) -> Result<Vec<u8>, Argon2Error> {
        let mut key = compute_argon2_kdf(password, salt, iterations, secret.len())?;
        assert_eq!(key.len(), secret.len());

        for (key_byte, secret_byte) in key.iter_mut().zip(secret.iter()) {
            *key_byte ^= secret_byte;
        }

        Ok(key)
    }

    fn lock(
        secret: &T,
        password: &[u8],
        iterations: u32,
        salt: Vec<u8>,
    ) -> Result<Self, Argon2Error> {
        let mut data = postcard::to_allocvec(&secret).map_err(|_| Argon2Error::MemoryTooLittle)?;
        let lock = Self::otp(&data, password, iterations, &salt)?;

        // Always overwrite unencrypted vector.
        for byte in data.iter_mut() {
            byte.clear();
        }

        Ok(Locked {
            lock,
            salt,
            iterations,
            phantom: PhantomData,
        })
    }

    fn create(
        secret: &T,
        password: &[u8],
        iterations: u32,
        salt_length: usize,
    ) -> Result<Self, Argon2Error> {
        let mut salt = vec![0; salt_length];
        OsRng.fill_bytes(salt.as_mut_slice());
        Self::lock(secret, password, iterations, salt)
    }

    pub fn into_otp_lock(self) -> OtpLock<T> {
        OtpLock::Locked(self)
    }
}

impl<T> Locked<T>
where
    for<'de> T: 'de + Clear + Deserialize<'de> + Serialize + Verify,
{
    /// Verifies integrity of data upon unlock.
    pub fn unlock(self, password: &[u8]) -> Result<Unlocked<T>, Locked<T>> {
        let unlocked = self.unlock_unchecked(password);
        match unlocked {
            Ok(unlocked) => {
                if unlocked.verify() {
                    Ok(unlocked)
                } else {
                    Err(unlocked.lock)
                }
            }
            err => err,
        }
    }
}

impl<T> IntoDatabaseValue for Locked<T>
where
    for<'de> T: 'de + Default + Deserialize<'de> + Serialize,
{
    fn database_byte_size(&self) -> usize {
        postcard::to_allocvec(self).unwrap().len()
    }

    fn copy_into_database(&self, bytes: &mut [u8]) {
        postcard::to_slice(self, bytes).unwrap();
    }
}

impl<T> FromDatabaseValue for Locked<T>
where
    for<'de> T: 'de + Default + Deserialize<'de> + Serialize,
{
    fn copy_from_database(bytes: &[u8]) -> io::Result<Self>
    where
        Self: Sized,
    {
        postcard::from_bytes(bytes).map_err(|e| std::io::Error::new(io::ErrorKind::Other, e))
    }
}

// Generic container
pub enum OtpLock<T>
where
    for<'de> T: 'de + Clear + Deserialize<'de> + Serialize,
{
    Unlocked(Unlocked<T>),
    Locked(Locked<T>),
}

impl<T> OtpLock<T>
where
    for<'de> T: 'de + Clear + Deserialize<'de> + Serialize,
{
    // Taken from Nimiq's JS implementation.
    // TODO: Adjust.
    pub const DEFAULT_ITERATIONS: u32 = 256;
    pub const DEFAULT_SALT_LENGTH: usize = 32;

    /// Calling code should make sure to clear the password from memory after use.
    pub fn new_unlocked(
        secret: T,
        password: &[u8],
        iterations: u32,
        salt_length: usize,
    ) -> Result<Self, Argon2Error> {
        Ok(OtpLock::Unlocked(Unlocked::new(
            secret,
            password,
            iterations,
            salt_length,
        )?))
    }

    /// Calling code should make sure to clear the password from memory after use.
    pub fn unlocked_with_defaults(secret: T, password: &[u8]) -> Result<Self, Argon2Error> {
        Self::new_unlocked(
            secret,
            password,
            Self::DEFAULT_ITERATIONS,
            Self::DEFAULT_SALT_LENGTH,
        )
    }

    /// Calling code should make sure to clear the password from memory after use.
    pub fn new_locked(
        secret: T,
        password: &[u8],
        iterations: u32,
        salt_length: usize,
    ) -> Result<Self, Argon2Error> {
        Ok(OtpLock::Locked(Locked::new(
            secret,
            password,
            iterations,
            salt_length,
        )?))
    }

    /// Calling code should make sure to clear the password from memory after use.
    pub fn locked_with_defaults(secret: T, password: &[u8]) -> Result<Self, Argon2Error> {
        Self::new_locked(
            secret,
            password,
            Self::DEFAULT_ITERATIONS,
            Self::DEFAULT_SALT_LENGTH,
        )
    }

    #[inline]
    pub fn is_locked(&self) -> bool {
        matches!(self, OtpLock::Locked(_))
    }

    #[inline]
    pub fn is_unlocked(&self) -> bool {
        !self.is_locked()
    }

    #[inline]
    #[must_use]
    pub fn lock(self) -> Self {
        match self {
            OtpLock::Unlocked(unlocked) => OtpLock::Locked(Unlocked::lock(unlocked)),
            l => l,
        }
    }

    #[inline]
    pub fn locked(self) -> Locked<T> {
        match self {
            OtpLock::Unlocked(unlocked) => Unlocked::lock(unlocked),
            OtpLock::Locked(locked) => locked,
        }
    }

    #[inline]
    pub fn unlocked(self) -> Result<Unlocked<T>, Self> {
        match self {
            OtpLock::Unlocked(unlocked) => Ok(unlocked),
            l => Err(l),
        }
    }

    #[inline]
    pub fn unlocked_ref(&self) -> Option<&Unlocked<T>> {
        match self {
            OtpLock::Unlocked(unlocked) => Some(unlocked),
            _ => None,
        }
    }
}
