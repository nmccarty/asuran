# Contributing to Asuran

We encourage anyone, regardless of skill level, to contribute. The worst that will happen is someone
coming along to mentor you in the right direction

## Contributions

Contributions should be made in the form of gitlab issues or pull requests.

Contributors' names will be added to [`HALL_OF_FAME.md`](HALL_OF_FAME.md).

For those submitting bug reports, your name will be added once your bug report is confirmed.

For those making code contributions, your name will be added once you land your first PR.

If you don't want your name added, just say so and we wont. 

## Getting started

As a general workflow, once you find something you want to work on, fork the repository, do the
work, then submit a PR for review. 

Our issues have five tags relating the expected difficulty. The first three directly relate the
expected difficulty and work load:

- [`D-EASY`](https://gitlab.com/asuran-rs/asuran/issues?label_name%5B%5D=D-EASY)

  These issues are expected to be easy to work on, requiring only basic understand of the project
  and rust in general, and not taking too much work to do.
  
- [`D-MEDIUM`](https://gitlab.com/asuran-rs/asuran/issues?label_name%5B%5D=D-MEDIUM)

  These issues are expected to take a not insubstantial amount of time to work, and more than a
  passing familiarity with the project.
  
- [`D-HARD`](https://gitlab.com/asuran-rs/asuran/issues?label_name%5B%5D=D-HARD)

  These issues are expected to take up large amounts of time or require in depth knowledge of the
  project and rust itself. 

The next two describe other aspect of the work and are good labels for new contributors to go by:

- [`Easy Tasks`](https://gitlab.com/asuran-rs/asuran/issues?label_name%5B%5D=Easy+Tasks)

  These issues may have a difficulty rating of medium or hard, however, they have individual
  independent tasks on them that, if they were their own issue, would be categorized as easy.
  
- [`Good First Issue`](https://gitlab.com/asuran-rs/asuran/issues?label_name%5B%5D=Good+First+Issue)

  These are issues expected to give a good introduction to the repository. They are most often easy
  issues that require little to no understanding of the project to start hacking on.
  
New contributors are encouraged to go for issues with the `Good First Issue` tag first, and then
move up to `D-EASY` as they grow more comfortable with the project. Though, this is not a hard
guideline, if you want to jump straight in, feel free to grab a `D-MEDIUM` or `D-HARD` and contact
@ThatOneLutenist or ask in one of the chats if you get stuck or need mentoring. 

## Primer on our development procedures 

Asuran development works on two major branches, `master` and `dev`. `master` is our 'stable' branch
where we apply strict code quality standards and all tests must pass all the time.

`dev` is a much looser experience, it is our "move fast and break things" branch. Code commuted to
dev may require further cleanup, like minor refactoring or addition of tests, before hitting master,
but it is still expected to eventually hit master. 

## Pull request check list

We rigorously verify all code committed to the `master` branch, but don't let that scare you. If you
aren't quite there yet, go ahead and submit your pull request. Someone will either help you get your
code up to where it needs to be to hit `master`, or pull it into the `dev` branch for cleanup by another
contributor. 

All contributions are welcome, don't feel like you have to write the most beautiful
code of your life before you hit that create pull request button. 

All pull requests should do the following things before landing on master:
- [ ] Compile
- [ ] Pass clippy 
- [ ] Pass all tests 
- [ ] Not reduce test coverage by a needless amount
- [ ] Have reasonably good doc comments
- [ ] Add tests for the new behavior if it is more than trivially complex

Contributions to the `dev` branch have a much simpler check list they must complete:
- [ ] Code must compile
- [ ] PR must be associated with a tracking issue

## Code of Conduct and Licensing

All contributers to this project are bound by the [Rust Code of
Conduct](https://www.rust-lang.org/policies/code-of-conduct). 

By contributing to this project, you agree to license your contribution under the MIT license.
