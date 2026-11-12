# Platform saved payments: one saved card, many independent businesses

Research date: 2026-11-12. Most vendor doc pages fetched show no "last updated" date; where a page had one, it is noted. All Stripe pages were fetched live on this date.

## Stripe Connect: saving on the platform and reusing at many connected accounts

**Takeaway:**
Stripe supports this directly. Save the card once on a platform Customer with a SetupIntent, then clone the PaymentMethod into each connected account and make a direct charge there, so each business is merchant of record. Cloning works only for `card` and `us_bank_account`, and the setup must meet the SCA rules of the connected account's country.

**Cited Findings:**

- Stripe's three charge types: direct charges (payment made directly to a connected account), and two indirect types, destination charges and separate charges and transfers (payment made to the platform, then moved to the connected account). The charge type decides whose information appears on the customer's statement and which account is debited for refunds and chargebacks. — [Stripe: How charges work](https://docs.stripe.com/connect/charges)
- Stripe recommends direct charges for SaaS platforms whose connected accounts "transact directly with their customers, who are often unaware of your platform's existence". Its examples are Shopify and Thinkific. — [Stripe: How charges work](https://docs.stripe.com/connect/charges)
- Direct charges need the `card_payments` capability. They "aren't recommended for legacy v1 Express and Custom accounts"; Stripe says to switch to v2 accounts or use destination charges with those. — [Stripe: How charges work](https://docs.stripe.com/connect/charges)
- Cloning is "only relevant when creating direct charges on connected accounts. It's not necessary when making charges on your platform account." With indirect charges the Customer and card already live on the platform. — [Stripe: Clone customers (legacy page)](https://docs.stripe.com/connect/cloning-customers-across-accounts)
- The old token-based cloning flow (Sources/Tokens API) is marked **Deprecated / legacy**. The current flow is "Share payment methods across multiple accounts". — [Stripe: Clone customers (legacy page)](https://docs.stripe.com/connect/cloning-customers-across-accounts)
- Current flow: create a Customer on the platform, create a SetupIntent on the platform, confirm it client side (this runs 3D Secure if needed), which attaches a PaymentMethod "for future use". "You only need to set up each payment method once." — [Stripe: Share payment methods across multiple accounts](https://docs.stripe.com/connect/direct-charges-multiple-accounts)
- Alternatively, take a first payment on the platform with a PaymentIntent and `setup_future_usage`, but those funds go to the platform's balance, not a connected account's. — [Stripe: Share payment methods](https://docs.stripe.com/connect/direct-charges-multiple-accounts)
- Clone by calling `POST /v1/payment_methods` with the `Stripe-Account` header set to the connected account, passing the platform Customer and PaymentMethod IDs. "Cloning supports PaymentMethods that have `type` set to either `card` or `us_bank_account`." — [Stripe: Share payment methods](https://docs.stripe.com/connect/direct-charges-multiple-accounts)
- "The cloned PaymentMethod is an independent object with a unique ID. It isn't linked to, or kept in sync with, the PaymentMethod on your platform. However, the cloned PaymentMethod inherits the setup performed on your platform account, so you don't need to set it up again on the connected account." — [Stripe: Share payment methods](https://docs.stripe.com/connect/direct-charges-multiple-accounts)
- Off-session charge on the connected account uses `off_session=true, confirm=true`. A clone not attached to a Customer is consumed by one charge; you clone again for the next charge, which "ensures that you always use the current platform version". — [Stripe: Share payment methods](https://docs.stripe.com/connect/direct-charges-multiple-accounts)
- For recurring charges at one connected account, create a Customer on that account and attach the clone. That Customer "has no association with the original object stored on your platform. You must update all instances to keep information updates in sync." — [Stripe: Share payment methods](https://docs.stripe.com/connect/direct-charges-multiple-accounts)
- SCA pitfall: "If your platform is in a different country than your connected accounts, the setup performed on your platform might not be sufficient." A US platform's setup "might not trigger the authentication required for direct charges in countries subject to SCA enforcement"; you may need `on_behalf_of` on the SetupIntent naming a connected account in that country. — [Stripe: Share payment methods](https://docs.stripe.com/connect/direct-charges-multiple-accounts)
- Mandate pitfall: "If a mandate is authorized for a PaymentIntent or SetupIntent on_behalf_of a connected account, you can't use that mandate with a different connected account." — [Stripe: Payment method Connect support](https://docs.stripe.com/payments/payment-methods/payment-method-connect-support)
- For ACH Direct Debit clones, Stripe duplicates the mandate to the connected account without new confirmation emails, and "you must communicate to the customer that their authorization extends to connected accounts on your platform", for example in the mandate terms, or risk confusion and disputes. — [Stripe: Payment method Connect support](https://docs.stripe.com/payments/payment-methods/payment-method-connect-support)
- Cash App Pay methods cannot be cloned across connected accounts that are business of record. SEPA Direct Debit cannot be cloned for destination charges; each connected account needs a new mandate via `on_behalf_of`. — [Stripe: Payment method Connect support](https://docs.stripe.com/payments/payment-methods/payment-method-connect-support)
- "The merchant of record determines the Stripe account authorized to create payments with a particular PaymentMethod." — [Stripe: Payment method Connect support](https://docs.stripe.com/payments/payment-methods/payment-method-connect-support)
- Indirect charges with `on_behalf_of` make the connected account the "business of record": settlement in its country, its fee structure, and its statement descriptor. — [Stripe: How charges work](https://docs.stripe.com/connect/charges)
- Platforms "often save a payment method for the customer, then use it for off session Destination `on_behalf_of` charges" to avoid repeated authentication (stated in the Cash App Pay section). — [Stripe: Payment method Connect support](https://docs.stripe.com/payments/payment-methods/payment-method-connect-support)

**Inferences:**

- Two Stripe paths keep one saved card usable at many businesses: (a) platform Customer plus clone plus direct charge, where each business is merchant of record on its own account, and (b) platform Customer plus destination charge with `on_behalf_of`, where the business is "business of record" for descriptor and settlement, but the charge, refund debit, and dispute debit sit on the platform balance. Path (a) matches Tetsu's "each business is its own merchant of record" model best.
- Separate charges and transfers without `on_behalf_of` make the platform the merchant of record, so it conflicts with Tetsu's model.
- With path (a), Tetsu holds the master Customer and must store the per-business consent itself. Clones never sync back, so card updates (new expiry, account updater) apply only to the platform object. Prefer one-shot clones per charge and avoid long-lived per-account copies except for subscriptions.
- Processor criterion to write now: "Supports saving a card once at the platform level and charging it, with customer consent, at any connected account that is merchant of record, including off-session, with SCA handled for the connected account's country."

**Gaps:**

- Stripe's docs do not say how a cloned card's network transaction ID or stored-credential flags carry into the connected account's charges. Behavior is implied ("inherits the setup") but not spelled out.
- Whether Accounts v2 changes the clone flow (for example a shared Customer configuration across accounts) was not checked.
- No Stripe statement on rate limits or caps on the number of clones.

## Stripe Link

**Takeaway:**
Link is a Stripe-run consumer wallet shared across Link-enabled merchants. The consumer's identity and cards belong to Stripe, not the platform. It speeds checkout but does not replace a platform-held saved card, and its support under Connect `on_behalf_of` was in private preview when last documented.

**Cited Findings:**

- Link saves the customer's chosen payment method "for future use across Link-enabled merchants". New customers opt in with a checkbox; the merchant sets its default state in Dashboard Link settings. Returning customers are recognized by email, phone, or browser cookie and confirm with a one-time passcode, then Link autofills saved details. — [Stripe: Link payment methods](https://docs.stripe.com/payments/link/link-payment-methods)
- Link payments show as type `link` or as `card` with `brand=link`, depending on integration. Some Link funding sources "don't support recurring payments or off-session charges". — [Stripe: Link payment methods](https://docs.stripe.com/payments/link/link-payment-methods)
- That page marks Link as "Available in: US". — [Stripe: Link payment methods](https://docs.stripe.com/payments/link/link-payment-methods)
- A search summary of Stripe's payment-method support page listed Link among methods supporting `on_behalf_of` "in private preview". I could not confirm the current status on the live page. — [Stripe: Payment method support](https://docs.stripe.com/payments/payment-methods/payment-method-support)
- Stripe docs say new customers can save a payment method to a specific business, to Link, or to both (search snippet, not fully verified). — [Stripe: Save customer payment methods](https://docs.stripe.com/payments/save-customer-payment-methods)

**Inferences:**

- Link gives the consumer a "pay anywhere on Stripe" experience without Tetsu building anything, but Tetsu cannot see or control that identity, and it covers only Stripe merchants. It is a useful complement, not a basis for Tetsu-owned cross-business profiles.

**Gaps:**

- No current Stripe page found that states Link support for direct charges on connected accounts, or whether a Link payment method can be cloned. Ask Stripe before relying on it.

## Other processors: Square, Braintree, Adyen, Helcim

**Takeaway:**
Square and Braintree both have an explicit "platform holds the card, charges it in a seller's account" feature (Square shared card on file; Braintree Shared Vault and Grant API). Adyen shares tokens between merchant accounts through Token Groups, but its platform model usually runs payments through the platform's merchant account. Helcim was not researched.

**Cited Findings:**

- **Square shared card on file:** "Store a card on file in your Square developer account and make payments with the card in other Square seller accounts." Steps: create a customer and card in the developer account, create or find a matching customer in the seller account, then create the payment in the seller account referencing the seller customer and the shared card. — [Square: Shared card walkthrough](https://developer.squareup.com/docs/cards-api/walkthrough-shared-card)
- Square needs seller OAuth scopes `CUSTOMERS_WRITE`, and `PAYMENTS_WRITE` plus `PAYMENTS_WRITE_SHARED_ONFILE` for the payment. The card must come from a Web Payments SDK or In-App Payments SDK token, and the postal code must match. — [Square: Shared card walkthrough](https://developer.squareup.com/docs/cards-api/walkthrough-shared-card)
- Square: "Always ask customers for permission before saving their card information." "Linking cards on file without obtaining customer permission can result in your application being disabled without notice." It suggests a checkbox in the purchase flow. — [Square: Shared card walkthrough](https://developer.squareup.com/docs/cards-api/walkthrough-shared-card)
- A Square forum thread reports a NOT_FOUND "Payment on file not found" error when using a shared card with a seller's customer ID. — [Square forum](https://developer.squareup.com/forums/t/create-a-payment-using-shared-cards-on-file-gets-not-found-payment-on-file-not-found-error/11769)
- **Braintree Shared Vault** lets a platform "store all credit cards and customer data in your own Braintree Vault" and create transactions for a connected merchant using the `shared_vault_transactions` right. The connected merchant can never update those payment methods or store them in its own Vault. "OAuth is in closed beta in production, and open beta in sandbox." — [Braintree: Shared Vault](https://developer.paypal.com/braintree/docs/guides/extend/oauth/shared-vault/)
- **Braintree Grant API** gives another merchant revocable access to a customer's card, debit card, or PayPal billing agreement via a nonce that expires after 24 hours if unused. A recipient cannot re-grant. — [Braintree: Grant API](https://developer.paypal.com/braintree/docs/reference/request/payment-method/grant/node)
- **Adyen Token Groups:** by default tokens work with one merchant account. Token Groups share "shopper references and their associated tokens" between merchant accounts under one company account; you contact Adyen Support to enable it. Details are stored only if the shopper consents. Tokens cannot be used for one-click with Apple Pay, Google Pay, or Samsung Pay. — [Adyen: Tokenization](https://docs.adyen.com/online-payments/tokenization)
- Adyen's classic platform model processes payments "through your Adyen merchant account" and pays out to account holders. Adyen's payment facilitator docs say the sub-merchant "is considered the Merchant of Record when a transaction is facilitated by you" (search snippet). — [Adyen: Classic platforms account structure](https://docs.adyen.com/classic-platforms/account-structure); [Adyen: Payment facilitators](https://docs.adyen.com/payment-methods/cards/payment-facilitators)

**Inferences:**

- In Adyen's platform model, one platform merchant account typically holds all tokens, so cross-business reuse is natural, but the platform is the processing merchant. Tetsu should confirm with Adyen how that maps to "each business is merchant of record".
- Square's flow matches Tetsu's model closely but only if every business connects through Tetsu's one OAuth app.
- Braintree's Shared Vault is beta-gated in production, a real adoption risk.

**Gaps:**

- Current Adyen for Platforms (non-classic) docs on token sharing across account holders were not found.
- Helcim and other processors were not researched within the call budget.
- Square country coverage and dispute handling for shared cards are not stated in the walkthrough.

## PCI DSS scope

**Takeaway:**
If card data is entered only into the processor's hosted fields, iframe, or redirect page, the platform can usually use SAQ A. Since PCI DSS v4.0.1, SAQ A for embedded iframes needs the merchant to confirm the page is not susceptible to script attacks.

**Cited Findings:**

- SAQ A r1 criterion: "The merchant has confirmed that their site is not susceptible to attacks from scripts that could affect the merchant's e-commerce system(s)." The PCI SSC post is dated 28 February 2025. — [PCI SSC blog: FAQ 1588](https://blog.pcisecuritystandards.org/faq-clarifies-new-saq-a-eligibility-criteria-for-e-commerce-merchants)
- It applies to pages that embed a TPSP's payment form, such as an iframe. Merchants meet it either with protections like Requirements 6.4.3 and 11.6.1, or with confirmation from their PCI DSS compliant TPSP that its solution includes script-attack protections when implemented as instructed. — [PCI SSC blog: FAQ 1588](https://blog.pcisecuritystandards.org/faq-clarifies-new-saq-a-eligibility-criteria-for-e-commerce-merchants)
- It does not apply to full redirects (HTTP, meta, or JavaScript) or to fully outsourced payment links. Merchants should confirm with their acquirer that SAQ A is right for them. — [PCI SSC blog: FAQ 1588](https://blog.pcisecuritystandards.org/faq-clarifies-new-saq-a-eligibility-criteria-for-e-commerce-merchants)
- Merchants who cannot meet the new criteria must use a fuller questionnaire such as SAQ A-EP (vendor summary). — [SecurityMetrics](https://marketing-webflow.securitymetrics.com/blog/big-changes-for-saq-a)
- A September 2026 post claims a June 2026 PCI SSC FAQ 1604 clarifies ASV scan scope for iframe pages. Not verified against PCI SSC. — [OneUptime](https://oneuptime.com/blog/post/2026-09-24-embedded-payment-iframe-saq-a-eligibility/view)

**Inferences:**

- Every approach above (Stripe clones, Square shared cards, Braintree vault, Adyen tokens) keeps card numbers in the processor's vault, so the platform's scope depends on how cards are collected, not on cross-business reuse. Holding only tokens does not add card data to Tetsu.
- Criterion: the processor must give written confirmation that its embedded fields include script-attack protections, so Tetsu can claim SAQ A.

**Gaps:**

- Who completes the SAQ when the connected business is merchant of record but Tetsu hosts the checkout page was not found in a primary source. Ask the chosen processor and acquirer.

## Consent, credential-on-file rules, and consumer UX

**Takeaway:**
Card network stored-credential rules require disclosure and consent before storing a card and a flagged initial transaction. Primary Visa or Mastercard texts were not retrieved, so the details below come from processor guides. Reusing a card at a different business than the one the consumer agreed with needs explicit consent that names that scope.

**Cited Findings:**

- The merchant must get "the cardholder's express, informed consent to the agreement before the initial transaction is processed", disclose how credentials will be used, and keep a record of the consent (processor and industry guides). — [Checkout.com: Visa stored credential framework](https://support.checkout.com/hc/en-us/articles/34282256010642-What-is-Visa-s-stored-credential-framework); [Merchant Risk Council](https://merchantriskcouncil.org/learning/resource-center/member-news/blog/2025/card-on-file-done-right-how-to-stay-compliant-with-the-stored-credential-mandate)
- The first transaction must be flagged as the initial credential-on-file transaction; Visa returns an identifier that later merchant-initiated transactions must reference. If the initial payment or verification is declined, the credential must not be stored. — [Braintree blog: Visa stored credential framework](https://origin-www.produswest2.braintreepayments.com/blog/visa-stored-credential-framework-processing-improvements-for-vaulted-cards/); [Visa Acceptance: Credentialed transactions guide](https://developer.visaacceptance.com/docs/vas/en-us/credentials/developer/vantivcnp/rest/credentials.html)
- Stripe requires telling ACH customers that their authorization "extends to connected accounts on your platform". — [Stripe: Payment method Connect support](https://docs.stripe.com/payments/payment-methods/payment-method-connect-support)
- Square requires asking permission before saving a card and suggests a checkbox. — [Square: Shared card walkthrough](https://developer.squareup.com/docs/cards-api/walkthrough-shared-card)
- Adyen tokens cannot be used for one-click with Apple Pay, Google Pay, or Samsung Pay. — [Adyen: Tokenization](https://docs.adyen.com/online-payments/tokenization)

**Inferences:**

- A safe UX: at save time, the consumer agrees that "this card may be used for bookings at businesses on Tetsu"; at each new business, the consumer picks the saved card in a customer-present checkout (a customer-initiated transaction), so no silent cross-business charge happens. Off-session charges (no-show fees, deposits) should rest on a per-business agreement shown at booking.
- Apple Pay and Google Pay give a similar "no re-entry" experience at every business with no platform-stored card, because the wallet lives on the consumer's device. This is a cheap baseline before cross-business saved cards.

**Gaps:**

- Visa Core Rules and Mastercard stored credential texts were not retrieved; a fetched Visa PDF copy was unreadable. The exact required agreement contents, and whether consent given to a platform covers several merchants of record, remain unconfirmed.
- Apple Pay and Google Pay support for each Connect charge type was not checked.

## Refunds, disputes, and statement descriptors

**Takeaway:**
On Stripe, direct charges put the business's descriptor on the statement and debit refunds and disputes from the business's balance. Destination and separate charges debit the platform, even with `on_behalf_of`, though `on_behalf_of` shows the business's descriptor.

**Cited Findings:**

- Direct charges: refunds and chargebacks reduce the connected account's balance. A refund pending for insufficient funds fails after 45 days. — [Stripe: How charges work](https://docs.stripe.com/connect/charges)
- Destination and separate charges: refunds debit the platform balance; the platform can reverse transfers to recover. Disputes on these charges, "with or without `on_behalf_of`", debit the platform balance for amount and fee. — [Stripe: How charges work](https://docs.stripe.com/connect/charges)
- For direct-charge disputes, Stripe can bill the dispute fee to the platform or connected account depending on configuration. With legacy Express or Custom accounts, "your platform is responsible for disputes and fraud." — [Stripe: How charges work](https://docs.stripe.com/connect/charges)
- Descriptor source: direct charge, connected account; destination or separate charge, platform; destination or separate with `on_behalf_of`, connected account (stated per payment method, for example Billie, Kriya, Mondu). — [Stripe: Payment method Connect support](https://docs.stripe.com/payments/payment-methods/payment-method-connect-support)
- Braintree Shared Vault transactions carry `facilitated_details` and `facilitator_details`; the page does not say whose descriptor appears. — [Braintree: Shared Vault](https://developer.paypal.com/braintree/docs/guides/extend/oauth/shared-vault/)

**Inferences:**

- With clone plus direct charge, the consumer sees the business's name, and the business owns refunds and disputes, which matches Tetsu's model. A dispute at one business does not touch the platform Customer's other uses, but the same card may be disputed more broadly if consumers do not recognize charges, so clear descriptors matter.

**Gaps:**

- Square and Adyen descriptor and dispute handling for shared cards were not found in the fetched pages.
