# Production Step CA image
#
# This Dockerfile wraps the official step-ca image for ECS deployment.
# CA configuration and secrets should be injected at runtime via:
# - AWS Secrets Manager for sensitive material (password, keys)
# - Environment variables or mounted volumes for config
#
# Build: docker build -f infra/stepca/Dockerfile -t ensure-pki-stepca:latest .

FROM smallstep/step-ca:0.30.1

ENV STEPPATH=/home/step

# The CA config, certs, and secrets must be mounted or injected at runtime.
# Expected structure:
#   /home/step/config/ca.json
#   /home/step/certs/root_ca.crt
#   /home/step/certs/intermediate_ca.crt
#   /home/step/secrets/password.txt
#   /home/step/secrets/intermediate_ca_key
#   /home/step/secrets/provisioner.key

USER step

CMD ["step-ca", "--password-file", "/home/step/secrets/password.txt", "/home/step/config/ca.json"]
