#[cfg(test)]
pub(crate) mod tests {
    use candle_core::{Device, Tensor};
    use snafu::{ensure_whatever, FromString, ResultExt, Whatever};

    use crate::architectures::{CausalLM, Decoder, Encoder, LayerOutputs};
    use crate::kv_cache::KeyValueCache;
    use crate::layers::attention::AttentionMask;
    use crate::models::hf::FromHFHub;
    use crate::util::device::tests::test_devices;
    use crate::util::tests::{assert_tensor_eq, IntoArrayD, PseudoRandomReduction};

    /// Sample transformer inputs used for most tests.
    pub fn sample_transformer_inputs(device: &Device) -> Result<(Tensor, AttentionMask), Whatever> {
        let input = Tensor::arange(0u32, 24, device)
            .and_then(|t| t.reshape((3, 8)))
            .whatever_context("Cannot create input tensor")?;

        let mask = Tensor::from_slice(
            &[
                1u8, 0, 1, 0, 0, 1, 0, 0, 1, 0, 0, 1, 0, 1, 1, 1, 0, 1, 1, 0, 0, 1, 1, 0,
            ],
            (3, 8),
            device,
        )
        .whatever_context("Cannot create attention mask tensor")?;

        Ok((
            input,
            AttentionMask::new(mask).whatever_context("Cannot create attention mask")?,
        ))
    }

    /// Check causal language model against test vectors.
    ///
    /// * `model_name` - The name of the model to test.
    /// * `model_revision` - The revision of the model to test.
    /// * `test_tensor` - The expected output tensor.
    ///   Shape: `(batch_size, sequence_length)`.
    pub fn check_causal_lm<C, M>(
        model_name: &str,
        model_revision: Option<&str>,
        test_tensor: impl IntoArrayD<f32>,
    ) -> Result<(), Whatever>
    where
        C: FromHFHub<Model = M>,
        M: CausalLM<Cache = KeyValueCache>,
    {
        let test_tensor = test_tensor
            .into_arrayd()
            .whatever_context("Cannot convert tensor")?;

        for device in test_devices() {
            let causal_lm = C::from_hf_hub(model_name, model_revision, &device)
                .whatever_context("Cannot load causal language model")?;

            let (input, mask) = sample_transformer_inputs(&device)?;

            let output = causal_lm
                .forward_t(&input, &mask, &mut KeyValueCache::no_cache(), None, false)
                .map_err(|e| Whatever::with_source(e, "Cannot decode input".to_string()))?;

            assert_tensor_eq::<f32>(
                output
                    .logits()
                    .pseudo_random_reduction()
                    .whatever_context("Cannot apply reduction using random vector")?,
                test_tensor.view(),
                1e-4,
            );
        }

        Ok(())
    }

    /// Check decoder against test vectors.
    ///
    /// * `model_name` - The name of the model to test.
    /// * `model_revision` - The revision of the model to test.
    /// * `test_tensor` - The expected output tensor.
    ///   Shape: `(batch_size, sequence_length)`.
    pub fn check_decoder<D, M>(
        model_name: &str,
        model_revision: Option<&str>,
        test_tensor: impl IntoArrayD<f32>,
    ) -> Result<(), Whatever>
    where
        D: FromHFHub<Model = M>,
        M: Decoder<Cache = KeyValueCache>,
    {
        let test_tensor = test_tensor
            .into_arrayd()
            .whatever_context("Cannot convert tensor")?;

        for device in test_devices() {
            let decoder = D::from_hf_hub(model_name, model_revision, &device)
                .whatever_context("Cannot load decoder")?;

            let (input, mask) = sample_transformer_inputs(&device)?;

            let output = decoder
                .forward_t(&input, &mask, &mut KeyValueCache::no_cache(), None, false)
                .map_err(|e| Whatever::with_source(e, "Cannot decode input".to_string()))?;

            ensure_whatever!(
                !output.layer_outputs().is_empty(),
                "Model did not have any outputs"
            );
            let last_output = output.layer_outputs().last().unwrap();

            assert_tensor_eq::<f32>(
                last_output
                    .pseudo_random_reduction()
                    .whatever_context("Cannot apply reduction using random vector")?,
                test_tensor.view(),
                1e-4,
            );
        }

        Ok(())
    }

    /// Check decoder with cache against test vectors.
    ///
    /// * `model_name` - The name of the model to test.
    /// * `model_revision` - The revision of the model to test.
    /// * `test_tensor` - The expected output tensor.
    ///   Shape: `(batch_size, sequence_length)`.
    pub fn check_decoder_with_cache<D, M>(
        model_name: &str,
        model_revision: Option<&str>,
        test_tensor: impl IntoArrayD<f32>,
    ) -> Result<(), Whatever>
    where
        D: FromHFHub<Model = M>,
        M: Decoder<Cache = KeyValueCache>,
    {
        let test_tensor = test_tensor
            .into_arrayd()
            .whatever_context("Cannot convert tensor")?;

        for device in test_devices() {
            let decoder = D::from_hf_hub(model_name, model_revision, &device)
                .whatever_context("Cannot load decoder")?;

            let (input, mask) = sample_transformer_inputs(&device)?;

            let mut cache = KeyValueCache::cache();
            let attention_mask = AttentionMask::new(
                mask.bool_mask()
                    .narrow(1, 0, 7)
                    .whatever_context("Cannot slice attention mask")?,
            )
            .whatever_context("Cannot build attention mask")?;

            decoder
                .forward_t(
                    &input
                        .narrow(1, 0, 7)
                        .whatever_context("Cannot slice input")?,
                    &attention_mask,
                    &mut cache,
                    None,
                    false,
                )
                .map_err(|e| Whatever::with_source(e, "Cannot decode input".to_string()))?;

            let attention_mask = attention_mask
                .extend(
                    &AttentionMask::new(
                        mask.bool_mask()
                            .narrow(1, 7, 1)
                            .whatever_context("Cannot slice attention mask")?,
                    )
                    .whatever_context("Cannot build attention mask")?,
                )
                .whatever_context("Cannot extend attention mask")?;

            let output = decoder
                .forward_t(
                    &input
                        .narrow(1, 7, 1)
                        .whatever_context("Cannot slice input")?,
                    &attention_mask,
                    &mut cache,
                    None,
                    false,
                )
                .map_err(|e| Whatever::with_source(e, "Cannot decode input".to_string()))?;

            ensure_whatever!(
                !output.layer_outputs().is_empty(),
                "Model did not have any outputs"
            );
            let last_output = output.layer_outputs().last().unwrap();

            assert_tensor_eq::<f32>(
                last_output
                    .pseudo_random_reduction()
                    .whatever_context("Cannot apply reduction using random vector")?,
                test_tensor.view(),
                1e-4,
            );
        }

        Ok(())
    }

    /// Check encoder against test vectors.
    ///
    /// * `model_name` - The name of the model to test.
    /// * `model_revision` - The revision of the model to test.
    /// * `test_tensor` - The expected output tensor.
    ///   Shape: `(batch_size, sequence_length)`.
    pub fn check_encoder<E, M>(
        model_name: &str,
        model_revision: Option<&str>,
        test_tensor: impl IntoArrayD<f32>,
    ) -> Result<(), Whatever>
    where
        E: FromHFHub<Model = M>,
        M: Encoder,
    {
        let test_tensor = test_tensor
            .into_arrayd()
            .whatever_context("Cannot convert tensor")?;

        for device in test_devices() {
            let encoder = E::from_hf_hub(model_name, model_revision, &device)
                .whatever_context("Cannot load encoder")?;

            let (input, mask) = sample_transformer_inputs(&device)?;

            let output = encoder
                .forward_t(&input, &mask, None, None, false)
                .map_err(|e| Whatever::with_source(e, "Cannot encode input".to_string()))?;

            ensure_whatever!(
                !output.layer_outputs().is_empty(),
                "Model did not have any outputs"
            );
            let last_output = output.layer_outputs().last().unwrap();

            assert_tensor_eq::<f32>(
                last_output
                    .pseudo_random_reduction()
                    .whatever_context("Cannot apply reduction using random vector")?,
                test_tensor.view(),
                1e-4,
            );
        }

        Ok(())
    }
}