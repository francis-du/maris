#![cfg(not(target_arch = "wasm32"))]

use candle_core::quantized::{
    gguf_file::{Content, Value, VersionedMagic},
    tokenizer::TokenizerFromGguf,
};
use std::collections::HashMap;
use tokenizers::Tokenizer;

fn gguf(tokens: &[&str]) -> Content {
    Content {
        magic: VersionedMagic::GgufV3,
        metadata: HashMap::from([
            (
                "tokenizer.ggml.model".to_owned(),
                Value::String("gpt2".to_owned()),
            ),
            (
                "tokenizer.ggml.pre".to_owned(),
                Value::String("qwen2".to_owned()),
            ),
            (
                "tokenizer.ggml.tokens".to_owned(),
                Value::Array(
                    tokens
                        .iter()
                        .map(|token| Value::String((*token).to_owned()))
                        .collect(),
                ),
            ),
            ("tokenizer.ggml.merges".to_owned(), Value::Array(Vec::new())),
        ]),
        tensor_infos: HashMap::new(),
        tensor_data_offset: 0,
    }
}

fn assert_vocab(tokenizer: &Tokenizer, tokens: &[&str]) {
    assert_eq!(tokenizer.get_vocab_size(true), tokens.len());
    for (id, token) in tokens.iter().enumerate() {
        assert_eq!(tokenizer.token_to_id(token), Some(id as u32));
        assert_eq!(tokenizer.id_to_token(id as u32).as_deref(), Some(*token));
    }
}

#[test]
fn gguf_token_types_filter_specials_but_keep_normal_and_byte_tokens() {
    let tokens = ["a", "b", "<unk>", "<control>", "<user>", "<unused>", "c"];
    let mut content = gguf(&tokens);
    let token_types = [1, 1, 2, 3, 4, 5, 6];
    content.metadata.insert(
        "tokenizer.ggml.token_type".to_owned(),
        Value::Array(token_types.into_iter().map(Value::U32).collect()),
    );

    let tokenizer = Tokenizer::from_gguf(&content).expect("token-type GGUF tokenizer");
    assert_vocab(&tokenizer, &tokens);
    let ids = [0, 2, 1, 3, 6, 4, 5];
    assert_eq!(
        tokenizer.decode(&ids, false).expect("decode all tokens"),
        "a<unk>b<control>c<user><unused>"
    );
    assert_eq!(
        tokenizer
            .decode(&ids, true)
            .expect("decode ordinary tokens"),
        "abc"
    );
    assert_eq!(
        tokenizer
            .encode("a<control>b", false)
            .expect("recognize registered special token")
            .get_ids(),
        &[0, 3, 1]
    );
}

#[test]
fn gguf_explicit_ids_register_specials_without_token_types() {
    let tokens = ["a", "b", "<bos>", "<eos>", "<pad>", "<sep>", "<unk>"];
    let mut content = gguf(&tokens);
    for (kind, id) in [("bos", 2), ("eos", 3), ("pad", 4), ("sep", 5), ("unk", 6)] {
        let key = format!("tokenizer.ggml.{kind}_token_id");
        content.metadata.insert(key, Value::U32(id));
    }
    for kind in ["bos", "eos"] {
        let key = format!("tokenizer.ggml.add_{kind}_token");
        content.metadata.insert(key, Value::Bool(true));
    }

    let tokenizer = Tokenizer::from_gguf(&content).expect("explicit-ID GGUF tokenizer");
    assert_vocab(&tokenizer, &tokens);
    let ids = [2, 0, 4, 1, 5, 6, 3];
    assert_eq!(
        tokenizer.decode(&ids, false).expect("decode all tokens"),
        "<bos>a<pad>b<sep><unk><eos>"
    );
    assert_eq!(
        tokenizer
            .decode(&ids, true)
            .expect("decode ordinary tokens"),
        "ab"
    );
    let encoded = tokenizer.encode("ab", true).expect("insert BOS and EOS");
    assert_eq!(encoded.get_ids(), &[2, 0, 1, 3]);
    assert_eq!(encoded.get_special_tokens_mask(), &[1, 0, 0, 1]);
    assert_eq!(
        tokenizer
            .decode(encoded.get_ids(), true)
            .expect("strip inserted BOS and EOS"),
        "ab"
    );
}
