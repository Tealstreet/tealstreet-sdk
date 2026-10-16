/// Stored inputs of the source abstract base. Inputs remain opaque: no cloning,
/// coercion, JSON materialization, or string narrowing occurs in this constructor.
pub struct BasePostprocessorContext<AccountName, ExchangeName, Options> {
    pub account_name: AccountName,
    pub exchange_name: ExchangeName,
    pub options: Options,
}
impl<AccountName, ExchangeName, Options>
    BasePostprocessorContext<AccountName, ExchangeName, Options>
{
    pub fn new(account_name: AccountName, exchange_name: ExchangeName, options: Options) -> Self {
        Self {
            account_name,
            exchange_name,
            options,
        }
    }
}

/// Source abstract process contract; concrete processors supply the implementation.
///
/// Omitting the source-required override must fail to compile:
/// ```compile_fail,E0046
/// use safe_cex_webworker::postprocessor_context::{BasePostprocessor, BasePostprocessorContext};
/// struct Missing { context: BasePostprocessorContext<(), (), ()> }
/// impl BasePostprocessor for Missing {
///     type AccountName = (); type ExchangeName = (); type Options = (); type Store = ();
///     fn context(&self) -> &BasePostprocessorContext<(), (), ()> { &self.context }
/// }
/// let _processor = Missing { context: BasePostprocessorContext::new((), (), ()) };
/// ```
pub trait BasePostprocessor {
    type AccountName;
    type ExchangeName;
    type Options;
    type Store;
    fn context(
        &self,
    ) -> &BasePostprocessorContext<Self::AccountName, Self::ExchangeName, Self::Options>;
    fn process(&mut self, store: &mut Self::Store);
}

pub struct ExchangeDependentPostprocessorContext<AccountName, ExchangeName, Options, Exchange> {
    pub account_name: AccountName,
    pub exchange_name: ExchangeName,
    pub options: Options,
    pub exchange: Exchange,
}
impl<AccountName, ExchangeName, Options, Exchange>
    ExchangeDependentPostprocessorContext<AccountName, ExchangeName, Options, Exchange>
{
    pub fn new(
        account_name: AccountName,
        exchange_name: ExchangeName,
        options: Options,
        exchange: Exchange,
    ) -> Self {
        Self {
            account_name,
            exchange_name,
            options,
            exchange,
        }
    }
}

/// The exchange-dependent base also requires a concrete process override.
/// ```compile_fail,E0046
/// use safe_cex_webworker::postprocessor_context::{ExchangeDependentPostprocessor, ExchangeDependentPostprocessorContext};
/// struct Missing { context: ExchangeDependentPostprocessorContext<(), (), (), ()> }
/// impl ExchangeDependentPostprocessor for Missing {
///     type AccountName = (); type ExchangeName = (); type Options = (); type Exchange = (); type Store = ();
///     fn context(&self) -> &ExchangeDependentPostprocessorContext<(), (), (), ()> { &self.context }
/// }
/// let _processor = Missing { context: ExchangeDependentPostprocessorContext::new((), (), (), ()) };
/// ```
pub trait ExchangeDependentPostprocessor {
    type AccountName;
    type ExchangeName;
    type Options;
    type Exchange;
    type Store;
    fn context(
        &self,
    ) -> &ExchangeDependentPostprocessorContext<
        Self::AccountName,
        Self::ExchangeName,
        Self::Options,
        Self::Exchange,
    >;
    fn process(&mut self, store: &mut Self::Store);
}
