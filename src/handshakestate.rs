extern crate rustc_serialize;

use constants::*;
use utils::*;
use crypto_types::*;
use cipherstate::*;
use handshakecryptoowner::*;
use symmetricstate::*;
use patterns::*;
use std::ops::DerefMut;
use self::rustc_serialize::hex::{FromHex, ToHex};

#[derive(Debug)]
pub enum NoiseError {
    InitError(&'static str),
    PrereqError(String),
    InputError(&'static str),
    StateError(&'static str),
    DecryptError
}

// TODO move has_* bools to just using Option<*>, but verify behavior is the same.
pub struct HandshakeState {
    rng : Box<RandomType>,                // for generating ephemerals
    symmetricstate : SymmetricState,      // for handshaking
    cipherstate1: Box<CipherStateType>,   // for I -> R transport msgs
    cipherstate2: Box<CipherStateType>,   // for I <- R transport msgs
    s: Box<DhType>,                       // local static
    e: Box<DhType>,                       // local ephemeral
    rs: Vec<u8>,                          // remote static
    re: Vec<u8>,                          // remote ephemeral
    handshake_pattern: HandshakePattern,
    has_s: bool,
    has_e: bool,
    has_rs: bool,
    has_re: bool,
    can_send: bool,
    message_patterns: Vec<Vec<Token>>, // 2D Token array
}

impl HandshakeState {
    pub fn new_from_owner<R: RandomType + Default + 'static,
        D: DhType + Default + 'static,
        C: CipherType + Default + 'static,
        H: HashType + Default + 'static>
    (owner: HandshakeCryptoOwner<R, D, C, H>,
     initiator: bool,
     handshake_pattern: HandshakePattern,
     prologue: &[u8],
     optional_preshared_key: Option<Vec<u8>>,
     cipherstate1: Box<CipherStateType>,
     cipherstate2: Box<CipherStateType>) -> Result<HandshakeState, NoiseError> {

        let dhlen = owner.s.pub_len();
        HandshakeState::new(
            Box::new(owner.rng),
            Box::new(owner.cipherstate),
            Box::new(owner.hasher),
            Box::new(owner.s), Box::new(owner.e),
            owner.rs[..dhlen].to_owned(),
            owner.re[..dhlen].to_owned(),
            owner.has_s, owner.has_e, owner.has_rs, owner.has_re,
            initiator, handshake_pattern, prologue, optional_preshared_key,
            cipherstate1, cipherstate2)
    }


    pub fn new(
            rng: Box<RandomType>,
            cipherstate: Box<CipherStateType>,
            hasher: Box<HashType>,
            s : Box<DhType>,
            e : Box<DhType>,
            rs: Vec<u8>,
            re: Vec<u8>,
            has_s: bool,
            has_e: bool,
            has_rs: bool,
            has_re: bool,
            initiator: bool,
            handshake_pattern: HandshakePattern,
            prologue: &[u8],
            optional_preshared_key: Option<Vec<u8>>,
            cipherstate1: Box<CipherStateType>,
            cipherstate2: Box<CipherStateType>) -> Result<HandshakeState, NoiseError> {
        use self::NoiseError::*;

        let mut handshake_name = String::with_capacity(128);

        if cipherstate1.name() != cipherstate2.name() {
            return Err(InitError("cipherstates don't match"));
        }

        if s.name() != e.name() {
            return Err(InitError("cipherstates don't match"));
        }

        if (has_s && has_e  && s.pub_len() != e.pub_len())
        || (has_s && has_rs && s.pub_len() >  rs.len())
        || (has_s && has_re && s.pub_len() >  re.len())
        {
            return Err(PrereqError(format!("key lengths aren't right. my pub: {}, their: {}", s.pub_len(), rs.len())));
        }

        handshake_name.push_str(match optional_preshared_key {
            Some(_) => "NoisePSK_",
            None    => "Noise_"
        });
        let tokens = resolve_handshake_pattern(handshake_pattern);
        handshake_name.push_str(&tokens.name);
        handshake_name.push('_');
        handshake_name.push_str(s.name());
        handshake_name.push('_');
        handshake_name.push_str(cipherstate.name());
        handshake_name.push('_');
        handshake_name.push_str(hasher.name());

        let mut symmetricstate = SymmetricState::new(cipherstate, hasher);

        symmetricstate.initialize(&handshake_name[..]);
        symmetricstate.mix_hash(prologue);

        if let Some(preshared_key) = optional_preshared_key { 
            symmetricstate.mix_preshared_key(&preshared_key);
        }

        if initiator {
            for token in tokens.premsg_pattern_i {
                match token {
                    Token::S => {assert!(has_s); symmetricstate.mix_hash(s.pubkey());},
                    Token::E => {assert!(has_e); symmetricstate.mix_hash(e.pubkey());},
                    _ => unreachable!()
                }
            }
            for token in tokens.premsg_pattern_r {
                match token {
                    Token::S => {assert!(has_rs); symmetricstate.mix_hash(&rs);},
                    Token::E => {assert!(has_re); symmetricstate.mix_hash(&re);},
                    _ => unreachable!()
                }
            }
        } else {
            for token in tokens.premsg_pattern_i {
                match token {
                    Token::S => {assert!(has_rs); symmetricstate.mix_hash(&rs);},
                    Token::E => {assert!(has_re); symmetricstate.mix_hash(&re);},
                    _ => unreachable!()
                }
            }
            for token in tokens.premsg_pattern_r {
                match token {
                    Token::S => {assert!(has_s); symmetricstate.mix_hash(s.pubkey());},
                    Token::E => {assert!(has_e); symmetricstate.mix_hash(e.pubkey());},
                    _ => unreachable!()
                }
            }
        }

        Ok(HandshakeState {
            rng: rng,  
            symmetricstate: symmetricstate,
            cipherstate1: cipherstate1,
            cipherstate2: cipherstate2,
            s: s, 
            e: e, 
            rs: rs, 
            re: re,
            has_s: has_s,
            has_e: has_e,
            has_rs: has_rs,
            has_re: has_re,
            handshake_pattern: handshake_pattern,
            can_send: initiator,
            message_patterns: tokens.msg_patterns,
        })
    }

    fn dh_len(&self) -> usize {
        self.s.pub_len()
    }

    fn dh(&mut self, local_s: bool, remote_s: bool) -> Result<(), NoiseError> {
        if !((!local_s  || self.has_s)  &&
             ( local_s  || self.has_e)  &&
             (!remote_s || self.has_rs) &&
             ( remote_s || self.has_re))
        {
            return Err(NoiseError::StateError("missing key material"))
        }

        let dh_len = self.dh_len();
        let mut dh_out = [0u8; MAXDHLEN];
        match (local_s, remote_s) {
            (true,  true)  => self.s.dh(&self.rs, &mut dh_out),
            (true,  false) => self.s.dh(&self.re, &mut dh_out),
            (false, true)  => self.e.dh(&self.rs, &mut dh_out),
            (false, false) => self.e.dh(&self.re, &mut dh_out),
        }
        self.symmetricstate.mix_key(&dh_out[..dh_len]);
        Ok(())
    }

    pub fn write_message(&mut self, 
                         payload: &[u8], 
                         message: &mut [u8]) -> Result<(usize, bool), NoiseError> {
        if !self.can_send {
            return Err(NoiseError::StateError("not ready to write messages yet."));
        }

        let next_tokens = if self.message_patterns.len() > 0 {
            Some(self.message_patterns.remove(0))
        } else {
            None
        };
        let last = next_tokens.is_some() && self.message_patterns.is_empty();

        let mut byte_index = 0;
        if let Some(tokens) = next_tokens {
            for token in &tokens {
                match *token {
                    Token::E => {
                        self.e.generate(self.rng.deref_mut());
                        let pubkey = self.e.pubkey();
                        copy_memory(pubkey, &mut message[byte_index..]);
                        byte_index += self.s.pub_len();
                        self.symmetricstate.mix_hash(&pubkey);
                        if self.symmetricstate.has_preshared_key() {
                            self.symmetricstate.mix_key(&pubkey);
                        }
                        self.has_e = true;
                    },
                    Token::S => {
                        if !self.has_s {
                            return Err(NoiseError::StateError("self.has_s is false"));
                        }
                        byte_index += self.symmetricstate.encrypt_and_hash(
                            &self.s.pubkey(),
                            &mut message[byte_index..]);
                    },
                    Token::Dhee => self.dh(false, false)?,
                    Token::Dhes => self.dh(false, true)?,
                    Token::Dhse => self.dh(true,  false)?,
                    Token::Dhss => self.dh(true,  true)?,
                }
            }
        }

        byte_index += self.symmetricstate.encrypt_and_hash(payload, &mut message[byte_index..]);
        if byte_index > MAXMSGLEN {
            return Err(NoiseError::InputError("with tokens, message size exceeds maximum"));
        }
        if last {
            self.symmetricstate.split(self.cipherstate1.deref_mut(), self.cipherstate2.deref_mut());
        }
        Ok((byte_index, last))
    }

    pub fn read_message(&mut self, 
                        message: &[u8], 
                        payload: &mut [u8]) -> Result<(usize, bool), NoiseError> {
        if message.len() > MAXMSGLEN {
            return Err(NoiseError::InputError("msg greater than max message length"));
        }

        let next_tokens = if self.message_patterns.len() > 0 {
            Some(self.message_patterns.remove(0))
        } else {
            None
        };
        let last = next_tokens.is_some() && self.message_patterns.is_empty();

        let dh_len = self.dh_len();
        let mut ptr = message;
        if let Some(tokens) = next_tokens {
            for token in &tokens {
                match *token {
                    Token::E => {
                        self.re.clear();
                        self.re.extend_from_slice(&ptr[..dh_len]);
                        ptr = &ptr[dh_len..];
                        self.symmetricstate.mix_hash(&self.re);
                        if self.symmetricstate.has_preshared_key() {
                            self.symmetricstate.mix_key(&self.re);
                        }
                        self.has_re = true;
                    },
                    Token::S => {
                        let data = if self.symmetricstate.has_key() {
                            let temp = &ptr[..dh_len + TAGLEN];
                            ptr = &ptr[dh_len + TAGLEN..];
                            temp
                        } else {
                            let temp = &ptr[..dh_len];
                            ptr = &ptr[dh_len..];
                            temp
                        };
                        self.rs.clear();
                        self.rs.resize(32, 0); // XXX
                        if !self.symmetricstate.decrypt_and_hash(data, &mut self.rs[..]) {
                            return Err(NoiseError::DecryptError);
                        }
                        self.has_rs = true;
                    },
                    Token::Dhee => self.dh(false, false)?,
                    Token::Dhes => self.dh(true, false)?,
                    Token::Dhse => self.dh(false, true)?,
                    Token::Dhss => self.dh(true, true)?,
                }
            }
        }
        if !self.symmetricstate.decrypt_and_hash(ptr, payload) {
            return Err(NoiseError::DecryptError);
        }
        self.can_send = !HandshakePattern::is_oneway(self.handshake_pattern);
        if last {
            self.symmetricstate.split(self.cipherstate1.deref_mut(), self.cipherstate2.deref_mut());
        }
        let payload_len = if self.symmetricstate.has_key() { ptr.len() - TAGLEN } else { ptr.len() };
        Ok((payload_len, last))
    }

}


