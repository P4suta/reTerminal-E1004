use cssparser::{
    AtRuleParser, CowRcStr, DeclarationParser, ParseError, Parser, ParserInput, ParserState,
    QualifiedRuleParser, RuleBodyItemParser, RuleBodyParser, StyleSheetParser, Token,
};

pub fn monochrome(css: &str) -> Result<(), &'static str> {
    let mut input = ParserInput::new(css);
    let mut parser = Parser::new(&mut input);
    for rule in StyleSheetParser::new(&mut parser, &mut StyleCheck) {
        rule.map_err(|_| "style exceeds the black-and-white design contract")?;
    }
    Ok(())
}

struct StyleCheck;

fn body<'i>(input: &mut Parser<'i, '_>) -> Result<(), ParseError<'i, ()>> {
    for rule in RuleBodyParser::new(input, &mut StyleCheck) {
        rule.map_err(|(error, _)| error)?;
    }
    Ok(())
}

fn tokens<'i>(
    input: &mut Parser<'i, '_>,
    values: bool,
    color_property: bool,
) -> Result<(), ParseError<'i, ()>> {
    while let Ok(token) = input.next_including_whitespace_and_comments().cloned() {
        match token {
            Token::Hash(value) | Token::IDHash(value) if values => {
                if !["000", "000000", "fff", "ffffff"]
                    .contains(&value.to_ascii_lowercase().as_str())
                {
                    return Err(input.new_custom_error(()));
                }
            }
            Token::Ident(value) if color_property => {
                let name = value.to_ascii_lowercase();
                if name == "transparent"
                    || cssparser::color::parse_named_color(&name)
                        .is_ok_and(|rgb| !matches!(rgb, (0, 0, 0) | (255, 255, 255)))
                {
                    return Err(input.new_custom_error(()));
                }
            }
            Token::Function(name) => {
                let name = name.to_ascii_lowercase();
                if values
                    && (name.contains("gradient")
                        || [
                            "rgb",
                            "rgba",
                            "hsl",
                            "hsla",
                            "hwb",
                            "lab",
                            "lch",
                            "oklab",
                            "oklch",
                            "color",
                            "color-mix",
                            "light-dark",
                            "device-cmyk",
                        ]
                        .contains(&name.as_str()))
                {
                    return Err(input.new_custom_error(()));
                }
                input.parse_nested_block(|input| tokens(input, values, color_property))?;
            }
            Token::CurlyBracketBlock | Token::ParenthesisBlock | Token::SquareBracketBlock => {
                input.parse_nested_block(|input| tokens(input, values, color_property))?;
            }
            Token::BadString(_)
            | Token::BadUrl(_)
            | Token::CloseParenthesis
            | Token::CloseSquareBracket
            | Token::CloseCurlyBracket => return Err(input.new_custom_error(())),
            _ => {}
        }
    }
    Ok(())
}

impl<'i> DeclarationParser<'i> for StyleCheck {
    type Declaration = ();
    type Error = ();
    fn parse_value<'t>(
        &mut self,
        name: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
        _: &ParserState,
    ) -> Result<(), ParseError<'i, ()>> {
        let name = name.to_ascii_lowercase();
        if matches!(
            name.as_str(),
            "opacity"
                | "box-shadow"
                | "text-shadow"
                | "filter"
                | "backdrop-filter"
                | "mix-blend-mode"
                | "background-blend-mode"
        ) {
            let valid = match input.next()?.clone() {
                Token::Number { value: 1.0, .. } => name == "opacity",
                Token::Ident(value) => {
                    if name.ends_with("blend-mode") {
                        value.eq_ignore_ascii_case("normal")
                    } else {
                        value.eq_ignore_ascii_case("none")
                    }
                }
                _ => false,
            };
            if !valid {
                return Err(input.new_custom_error(()));
            }
            let _ = input.try_parse(cssparser::parse_important);
            input.expect_exhausted()?;
            return Ok(());
        }
        if matches!(
            name.as_str(),
            "color"
                | "background"
                | "background-color"
                | "outline-color"
                | "accent-color"
                | "caret-color"
                | "fill"
                | "stroke"
        ) {
            let state = input.state();
            let valid = match input.next()?.clone() {
                Token::Hash(value) | Token::IDHash(value) => ["000", "000000", "fff", "ffffff"]
                    .contains(&value.to_ascii_lowercase().as_str()),
                Token::Ident(value) => {
                    value.eq_ignore_ascii_case("black")
                        || value.eq_ignore_ascii_case("white")
                        || (matches!(name.as_str(), "fill" | "stroke")
                            && (value.eq_ignore_ascii_case("none")
                                || value.eq_ignore_ascii_case("currentcolor")))
                }
                _ => false,
            };
            if !valid {
                return Err(input.new_custom_error(()));
            }
            input.reset(&state);
        }
        let color_property = name.contains("color")
            || name.starts_with("border")
            || matches!(
                name.as_str(),
                "background" | "outline" | "text-decoration" | "column-rule" | "fill" | "stroke"
            );
        tokens(input, true, color_property)
    }
}

impl<'i> QualifiedRuleParser<'i> for StyleCheck {
    type Prelude = ();
    type QualifiedRule = ();
    type Error = ();
    fn parse_prelude<'t>(&mut self, input: &mut Parser<'i, 't>) -> Result<(), ParseError<'i, ()>> {
        tokens(input, false, false)
    }
    fn parse_block<'t>(
        &mut self,
        _: (),
        _: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<(), ParseError<'i, ()>> {
        body(input)
    }
}

impl<'i> AtRuleParser<'i> for StyleCheck {
    type Prelude = ();
    type AtRule = ();
    type Error = ();
    fn parse_prelude<'t>(
        &mut self,
        _: CowRcStr<'i>,
        input: &mut Parser<'i, 't>,
    ) -> Result<(), ParseError<'i, ()>> {
        tokens(input, false, false)
    }
    fn parse_block<'t>(
        &mut self,
        _: (),
        _: &ParserState,
        input: &mut Parser<'i, 't>,
    ) -> Result<(), ParseError<'i, ()>> {
        body(input)
    }
}

impl<'i> RuleBodyItemParser<'i, (), ()> for StyleCheck {
    fn parse_declarations(&self) -> bool {
        true
    }
    fn parse_qualified(&self) -> bool {
        true
    }
}
