FROM alpine:3.20
RUN apk add --no-cache curl jq openssl bash
COPY publish_cards.sh /usr/local/bin/publish_cards.sh
RUN chmod +x /usr/local/bin/publish_cards.sh
ENTRYPOINT ["publish_cards.sh"]
