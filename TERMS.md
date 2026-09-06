# Terms of Use — Swept

**Version 1.0.** Applies to the official Swept binaries distributed by
__LEGAL_ENTITY__ ("we", "us"). Last revised 2026-09-06.

---

## 0. What this document is, and what it is not

Swept's **source code** is licensed under the MIT License — see
[`LICENSE`](LICENSE). Nothing here narrows that grant. You may use, copy,
modify, merge, publish, distribute, sublicense and sell the source exactly as
the MIT License permits, and you do not need to accept this document to do any
of it.

This document governs **the official signed, notarized builds we distribute**
(the `.app`, the `.dmg`, and any binary published under our Developer ID). It
exists to state, in specific terms and where you will actually read them, what
this program does and what you are accepting when you run it.

If the two ever conflict as to the source code, the MIT License wins.

> **Read section 2 and section 5 even if you read nothing else.** This program
> removes files from your computer. That is its purpose, not a failure mode.

---

## 1. What Swept does

Swept identifies files on your Mac that are commonly safe to remove — caches,
logs, build artifacts, browser data, items left behind by uninstalled
applications — and, on your explicit instruction, moves them to the Trash.

By design it:

- **previews before it acts.** Every disposal is shown to you with its path and
  size before anything happens.
- **never acts on its own.** There is no automatic, scheduled, or background
  cleaning. Nothing is removed without a confirmation you gave for that run.
- **prefers the Trash to deletion.** Recoverable disposal is the default;
  irreversible deletion requires a separate, explicit instruction.
- **writes down what it did.** Every planned and executed action is appended to
  `~/Library/Application Support/swept/audit.jsonl`.

These are real properties of the program, tested and enforced in code. They are
**not a guarantee that the program is free of defects**, and they are not a
substitute for your own backups.

---

## 2. Assumption of risk

**You are operating a tool that destroys data.** By installing or using Swept
you acknowledge and accept that:

1. **File removal is not reliably reversible.** Items moved to the Trash can be
   restored until the Trash is emptied. Items removed permanently cannot be
   restored by us, by Swept, or by macOS.
2. **You alone decide what is removed.** Swept proposes; you confirm. The
   decision to confirm a disposal, and the consequences of that decision, are
   yours.
3. **Software has defects.** Despite testing, a safety kernel, and a preview
   step, Swept may misidentify a file, misreport a size, or behave unexpectedly
   on a configuration we have not seen. Some defects are discovered only after
   they have caused harm.
4. **Removing a file can break things that depend on it.** Clearing caches can
   sign you out of websites, discard offline data, or force applications to
   rebuild state. Clearing browser history erases it permanently. Setting a
   login item aside can stop software from starting.
5. **We cannot recover your data.** We have no copy of it and no ability to
   retrieve it.

You accept these risks knowingly and voluntarily, and you agree that using
Swept is your decision to make and your responsibility to bear.

---

## 3. Back up before you use this

**Maintain a current, tested backup before running Swept.** Time Machine, a
bootable clone, or any backup you have actually verified you can restore from.

This is a condition of use, not a suggestion. If losing a file would matter to
you, that file must exist somewhere Swept cannot reach before you run it.

---

## 4. DISCLAIMER OF WARRANTIES

THE SOFTWARE IS PROVIDED "AS IS" AND "AS AVAILABLE", WITH ALL FAULTS AND
WITHOUT WARRANTY OF ANY KIND, EXPRESS, IMPLIED, STATUTORY OR OTHERWISE. TO THE
MAXIMUM EXTENT PERMITTED BY APPLICABLE LAW, WE EXPRESSLY DISCLAIM ALL
WARRANTIES, INCLUDING WITHOUT LIMITATION THE IMPLIED WARRANTIES OF
**MERCHANTABILITY**, **FITNESS FOR A PARTICULAR PURPOSE**, **TITLE**, **QUIET
ENJOYMENT**, **DATA ACCURACY**, AND **NON-INFRINGEMENT**, AND ANY WARRANTIES
ARISING FROM COURSE OF DEALING, COURSE OF PERFORMANCE, OR USAGE OF TRADE.

WE DO NOT WARRANT THAT: THE SOFTWARE WILL MEET YOUR REQUIREMENTS; THAT IT WILL
OPERATE UNINTERRUPTED, SECURELY, OR ERROR-FREE; THAT ANY FILE IT IDENTIFIES IS
IN FACT SAFE TO REMOVE; THAT ANY SIZE, COUNT OR SAVING IT REPORTS IS ACCURATE;
THAT DEFECTS WILL BE CORRECTED; OR THAT ANY DATA REMOVED CAN BE RECOVERED.

NO ADVICE OR INFORMATION, WHETHER ORAL OR WRITTEN, OBTAINED FROM US OR THROUGH
THE SOFTWARE, CREATES ANY WARRANTY NOT EXPRESSLY STATED HERE.

---

## 5. LIMITATION OF LIABILITY

TO THE MAXIMUM EXTENT PERMITTED BY APPLICABLE LAW, IN NO EVENT AND UNDER NO
LEGAL THEORY — WHETHER IN CONTRACT, TORT (INCLUDING NEGLIGENCE), STRICT
LIABILITY, WARRANTY, OR OTHERWISE — SHALL __LEGAL_ENTITY__, ITS MEMBERS,
MANAGERS, OFFICERS, EMPLOYEES, AGENTS, CONTRIBUTORS OR LICENSORS BE LIABLE TO
YOU OR ANY THIRD PARTY FOR:

**(a)** ANY **LOSS OF, CORRUPTION OF, OR INABILITY TO RECOVER DATA, FILES,
DOCUMENTS, PHOTOGRAPHS, PROJECTS, SOURCE CODE, CONFIGURATION, CREDENTIALS,
BROWSING HISTORY, OR ANY OTHER CONTENT**, HOWEVER CAUSED;

**(b)** ANY **INDIRECT, INCIDENTAL, SPECIAL, EXEMPLARY, PUNITIVE OR
CONSEQUENTIAL DAMAGES** OF ANY CHARACTER;

**(c)** ANY **LOST PROFITS, LOST REVENUE, LOST SAVINGS, LOSS OF GOODWILL, WORK
STOPPAGE, BUSINESS INTERRUPTION, COMPUTER FAILURE OR MALFUNCTION, DEVICE
DAMAGE, COST OF SUBSTITUTE GOODS OR SERVICES, OR COST OF DATA RECOVERY**; OR

**(d)** ANY DAMAGES ARISING OUT OF OR IN CONNECTION WITH THE USE OF, OR THE
INABILITY TO USE, THE SOFTWARE;

**EVEN IF WE HAVE BEEN ADVISED OF THE POSSIBILITY OF SUCH DAMAGES, AND EVEN IF
A LIMITED REMEDY IS FOUND TO HAVE FAILED OF ITS ESSENTIAL PURPOSE.**

**AGGREGATE CAP.** OUR TOTAL CUMULATIVE LIABILITY TO YOU FOR ALL CLAIMS ARISING
OUT OF OR RELATING TO THE SOFTWARE OR THESE TERMS SHALL NOT EXCEED THE GREATER
OF **(i)** THE AMOUNT YOU ACTUALLY PAID US FOR THE SOFTWARE IN THE TWELVE
MONTHS PRECEDING THE CLAIM — **WHICH, FOR THE FREE OFFICIAL BUILDS, IS ZERO
DOLLARS (US$0.00)** — OR **(ii)** **FIFTY UNITED STATES DOLLARS (US$50.00)**.

THE DISCLAIMERS AND LIMITATIONS IN SECTIONS 4 AND 5 ARE A FUNDAMENTAL BASIS OF
THE BARGAIN BETWEEN US. THE SOFTWARE IS PROVIDED FREE OF CHARGE, AND WE WOULD
NOT PROVIDE IT ON ANY OTHER TERMS.

---

## 6. Your responsibilities

You agree that you will:

- maintain current backups, as required by section 3;
- review each preview before confirming it, and confirm only what you intend to
  remove;
- use Swept only on computers and data you own or are authorised to administer;
- not rely on Swept as the sole safeguard for anything you cannot afford to
  lose; and
- comply with all applicable laws in your use of the Software.

**If you deploy Swept for others** — across an organisation's machines, or on
behalf of clients — you accept responsibility for that deployment, for the
backups protecting it, and for the instructions you give the people affected by
it, and you agree to indemnify and hold us harmless from claims arising out of
that deployment.

---

## 7. No professional advice

Swept's findings, recommendations and figures are automated output, not
professional advice. Nothing it reports is a warranty that a file is safe to
remove, that your system is healthy, or that a security or privacy concern has
been resolved.

---

## 8. Third-party software, trademarks and privacy

See [`NOTICE.md`](NOTICE.md) for trademark attributions and third-party
components, and [`PRIVACY.md`](PRIVACY.md) for what the Software does and does
not collect.

Swept is an independent project. It is **not affiliated with, endorsed by, or
sponsored by** Apple Inc., MacPaw Inc., or any browser or application vendor
whose data it can act upon.

---

## 9. Where these limits may not apply to you

Some jurisdictions do not allow the exclusion of implied warranties, or the
exclusion or limitation of liability for incidental, consequential or certain
other damages. **In those jurisdictions, the exclusions and limitations in
sections 4 and 5 apply only to the maximum extent that law permits, and you may
have rights that those sections cannot take away.**

Nothing in these Terms excludes or limits liability for death or personal
injury caused by negligence, for fraud or fraudulent misrepresentation, or for
any other liability that cannot lawfully be excluded.

---

## 10. General

**Severability.** If any provision is held unenforceable, it is modified to the
minimum extent necessary to make it enforceable, or severed if it cannot be;
the remainder stays in force. In particular, if any part of section 5 is held
unenforceable, the remaining limitations continue to apply.

**Governing law.** These Terms are governed by the laws of the State of
__GOVERNING_STATE__, United States, without regard to its conflict-of-laws
rules. The United Nations Convention on Contracts for the International Sale of
Goods does not apply.

**Entire agreement.** These Terms, together with the MIT License as it applies
to the source code, are the entire agreement between you and us regarding the
Software.

**No waiver.** Our failure to enforce any provision is not a waiver of it.

**Changes.** We may revise these Terms for future releases. Revisions apply to
the version of the Software they ship with; they do not change the terms of a
build you already have. The version at the top of this document identifies
which revision a given build presented to you.

---

## 11. Acknowledgement

The official builds of Swept ask you to confirm, on first launch, that you have
read and accepted these Terms and that you maintain backups. That confirmation
is recorded locally in
`~/Library/Application Support/swept/acceptance.json` — on your machine only.
It is never transmitted anywhere. See [`PRIVACY.md`](PRIVACY.md).

**If you do not accept these Terms, do not install or use the official builds.**
The source code remains available to you under the MIT License regardless.
